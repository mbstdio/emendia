//! Bounded X11 selection transfers and a persistent, multi-format clipboard owner.
//! The owner pumps requests independently of capture jobs and GPUI.
use anyhow::{Context, Result, bail};
use std::{
    collections::{HashMap, VecDeque},
    sync::{OnceLock, mpsc},
    thread,
    time::{Duration, Instant},
};
use x11rb::{
    CURRENT_TIME, NONE,
    connection::Connection,
    protocol::{
        Event,
        xfixes::{self, ConnectionExt as _},
        xproto::{
            self, Atom, AtomEnum, ChangeWindowAttributesAux, ConnectionExt as _, CreateWindowAux,
            EventMask, PropMode, Property, SelectionNotifyEvent, Window, WindowClass,
        },
    },
    rust_connection::RustConnection,
    wrapper::ConnectionExt as _,
};

const LIMIT: usize = 16 * 1024 * 1024;
const CHUNK: usize = 32 * 1024;
const TIMEOUT: Duration = Duration::from_millis(1200);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Stamp {
    pub owner: Window,
    timestamp: u32,
}

#[derive(Clone, Debug)]
pub(super) struct Format {
    pub(super) target: Atom,
    pub(super) type_: Atom,
    pub(super) format: u8,
    pub(super) data: Vec<u8>,
}

pub(super) struct Reader {
    pub connection: RustConnection,
    pub window: Window,
    pub clipboard: Atom,
    property: Atom,
    targets: Atom,
    incr: Atom,
    stamp: Stamp,
    pending: VecDeque<Event>,
}

pub(super) fn atom(connection: &RustConnection, name: &str) -> Result<Atom> {
    Ok(connection
        .intern_atom(false, name.as_bytes())?
        .reply()?
        .atom)
}

impl Reader {
    pub fn new() -> Result<Self> {
        let (connection, screen) = x11rb::connect(None)?;
        let window = connection.generate_id()?;
        connection
            .create_window(
                x11rb::COPY_DEPTH_FROM_PARENT,
                window,
                connection.setup().roots[screen].root,
                0,
                0,
                1,
                1,
                0,
                WindowClass::INPUT_OUTPUT,
                0,
                &CreateWindowAux::new().event_mask(EventMask::PROPERTY_CHANGE),
            )?
            .check()?;
        let clipboard = atom(&connection, "CLIPBOARD")?;
        connection.xfixes_query_version(5, 0)?.reply()?;
        connection
            .xfixes_select_selection_input(
                window,
                clipboard,
                xfixes::SelectionEventMask::SET_SELECTION_OWNER
                    | xfixes::SelectionEventMask::SELECTION_WINDOW_DESTROY
                    | xfixes::SelectionEventMask::SELECTION_CLIENT_CLOSE,
            )?
            .check()?;
        let mut reader = Self {
            property: atom(&connection, "_EMENDIA_SELECTION")?,
            targets: atom(&connection, "TARGETS")?,
            incr: atom(&connection, "INCR")?,
            connection,
            window,
            clipboard,
            stamp: Stamp::default(),
            pending: VecDeque::new(),
        };
        reader.stamp()?;
        // XFixes reports subsequent changes, but does not announce ownership that
        // predates this subscription. Recover its timestamp through ICCCM.
        if reader.stamp.owner != NONE && reader.stamp.timestamp == 0 {
            let timestamp = atom(&reader.connection, "TIMESTAMP")?;
            if let Ok(value) = reader.load(timestamp)
                && value.format == 32
                && let Some(bytes) = value.data.as_chunks::<4>().0.first()
            {
                reader.stamp.timestamp = u32::from_ne_bytes(*bytes);
            }
        }
        Ok(reader)
    }

    fn note(&mut self, event: &Event) {
        if let Event::XfixesSelectionNotify(event) = event
            && event.selection == self.clipboard
        {
            self.stamp = Stamp {
                owner: event.owner,
                timestamp: event.selection_timestamp,
            };
        }
    }

    pub fn stamp(&mut self) -> Result<Stamp> {
        // A reply is a server round trip: notifications already queued by earlier
        // ownership changes are visible before we compare or restore a snapshot.
        let owner = self
            .connection
            .get_selection_owner(self.clipboard)?
            .reply()?
            .owner;
        while let Some(event) = self.connection.poll_for_event()? {
            self.note(&event);
            if matches!(event, Event::SelectionRequest(_) | Event::PropertyNotify(_)) {
                self.pending.push_back(event);
            }
        }
        self.stamp.owner = owner;
        Ok(self.stamp)
    }

    pub fn wait_for_copy(
        &mut self,
        before: Stamp,
        check: impl Fn() -> Result<()>,
    ) -> Result<Stamp> {
        let deadline = Instant::now() + TIMEOUT;
        loop {
            check()?;
            let stamp = self.stamp()?;
            if stamp != before && stamp.owner != NONE {
                return Ok(stamp);
            }
            if Instant::now() >= deadline {
                bail!(crate::i18n::t(
                    "No text copied. Check the selection and Ctrl+C support."
                ));
            }
            thread::sleep(Duration::from_millis(10));
        }
    }

    fn load(&mut self, target: Atom) -> Result<Format> {
        self.connection
            .delete_property(self.window, self.property)?
            .check()?;
        let sequence = self
            .connection
            .convert_selection(
                self.window,
                self.clipboard,
                target,
                self.property,
                CURRENT_TIME,
            )?
            .sequence_number();
        self.connection.flush()?;
        let deadline = Instant::now() + TIMEOUT;
        let mut result: Option<Format> = None;
        let mut incremental = false;
        loop {
            if Instant::now() >= deadline {
                bail!("Clipboard transfer timed out");
            }
            let Some((event, event_sequence)) = self.connection.poll_for_event_with_sequence()?
            else {
                thread::sleep(Duration::from_millis(5));
                continue;
            };
            self.note(&event);
            if event_sequence < sequence {
                continue;
            }
            let ready = match event {
                Event::SelectionNotify(event)
                    if event.requestor == self.window
                        && event.selection == self.clipboard
                        && event.target == target =>
                {
                    if event.property == NONE {
                        bail!("Clipboard format is unavailable");
                    }
                    true
                }
                Event::PropertyNotify(event)
                    if incremental
                        && event.window == self.window
                        && event.atom == self.property
                        && event.state == Property::NEW_VALUE =>
                {
                    true
                }
                _ => false,
            };
            if !ready {
                continue;
            }
            let reply = self
                .connection
                .get_property(
                    true,
                    self.window,
                    self.property,
                    AtomEnum::ANY,
                    0,
                    (LIMIT / 4) as u32,
                )?
                .reply()?;
            if reply.type_ == self.incr {
                incremental = true;
                self.connection
                    .delete_property(self.window, self.property)?
                    .check()?;
                continue;
            }
            if ![8, 16, 32].contains(&reply.format) || reply.bytes_after != 0 {
                bail!("Clipboard format is invalid or exceeds 16 MiB");
            }
            let value = result.get_or_insert_with(|| Format {
                target,
                type_: reply.type_,
                format: reply.format,
                data: Vec::new(),
            });
            if value.type_ != reply.type_
                || value.format != reply.format
                || value.data.len() + reply.value.len() > LIMIT
            {
                bail!("Clipboard transfer changed format or exceeds 16 MiB");
            }
            let finished = !incremental || reply.value.is_empty();
            value.data.extend(reply.value);
            if finished {
                return Ok(result.unwrap());
            }
        }
    }

    pub fn snapshot(&mut self) -> Result<(Stamp, Vec<Format>)> {
        let before = self.stamp()?;
        if before.owner == NONE {
            return Ok((before, Vec::new()));
        }
        let targets = self.load(self.targets)?;
        if targets.type_ != u32::from(AtomEnum::ATOM) || targets.format != 32 {
            bail!("Invalid clipboard target list");
        }
        let ignored = [
            "TARGETS",
            "TIMESTAMP",
            "MULTIPLE",
            "SAVE_TARGETS",
            "INCR",
            "DELETE",
            "INSERT_SELECTION",
            "INSERT_PROPERTY",
        ]
        .into_iter()
        .map(|name| atom(&self.connection, name))
        .collect::<Result<Vec<_>>>()?;
        let mut formats = Vec::new();
        let mut size = 0;
        for bytes in targets.data.as_chunks::<4>().0 {
            let target = u32::from_ne_bytes(*bytes);
            if ignored.contains(&target)
                || formats.iter().any(|value: &Format| value.target == target)
            {
                continue;
            }
            if formats.len() >= 32 {
                bail!("Too many clipboard formats to preserve");
            }
            let value = self
                .load(target)
                .context("Unable to preserve a clipboard format; capture cancelled")?;
            size += value.data.len();
            if size > LIMIT {
                bail!("Clipboard snapshot exceeds 16 MiB; capture cancelled");
            }
            formats.push(value);
        }
        if self.stamp()? != before {
            bail!("The clipboard changed during capture; try again");
        }
        Ok((before, formats))
    }

    pub fn text(&mut self) -> Result<String> {
        for name in ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain"] {
            let target = atom(&self.connection, name)?;
            if let Ok(value) = self.load(target)
                && value.format == 8
            {
                return String::from_utf8(value.data).context("Selection is not valid UTF-8");
            }
        }
        let value = self.load(AtomEnum::STRING.into())?;
        if value.format != 8 {
            bail!("Selection is not text");
        }
        Ok(value.data.into_iter().map(char::from).collect())
    }
}

struct Command {
    formats: Vec<Format>,
    expected: Option<Stamp>,
    reply: mpsc::SyncSender<std::result::Result<Stamp, String>>,
}

struct Owner {
    commands: mpsc::Sender<Command>,
}
static OWNER: OnceLock<std::result::Result<Owner, String>> = OnceLock::new();

pub(super) fn put(formats: Vec<Format>, expected: Option<Stamp>) -> Result<Stamp> {
    let owner = OWNER
        .get_or_init(|| {
            let (commands, receiver) = mpsc::channel();
            let (ready, initialized) = mpsc::sync_channel(1);
            thread::Builder::new()
                .name("emendia-clipboard".into())
                .spawn(move || {
                    let mut reader = match Reader::new() {
                        Ok(reader) => {
                            let _ = ready.send(Ok(()));
                            reader
                        }
                        Err(error) => {
                            let _ = ready.send(Err(error.to_string()));
                            return;
                        }
                    };
                    if let Err(error) = serve(&mut reader, receiver) {
                        tracing::error!(%error, "Clipboard owner stopped");
                    }
                })
                .map_err(|error| error.to_string())?;
            initialized.recv().map_err(|error| error.to_string())??;
            Ok(Owner { commands })
        })
        .as_ref()
        .map_err(|error| anyhow::anyhow!(error.clone()))?;
    let (reply, result) = mpsc::sync_channel(1);
    owner.commands.send(Command {
        formats,
        expected,
        reply,
    })?;
    result
        .recv_timeout(Duration::from_secs(3))?
        .map_err(anyhow::Error::msg)
}

pub(super) fn put_text(text: &str) -> Result<Stamp> {
    let (connection, _) = x11rb::connect(None)?;
    let mut formats = Vec::new();
    for name in ["UTF8_STRING", "text/plain;charset=utf-8", "text/plain"] {
        let target = atom(&connection, name)?;
        formats.push(Format {
            target,
            type_: target,
            format: 8,
            data: text.as_bytes().to_vec(),
        });
    }
    put(formats, None)
}

struct Transfer {
    requestor: Window,
    property: Atom,
    value: Format,
    offset: usize,
    deadline: Instant,
}

fn serve(reader: &mut Reader, commands: mpsc::Receiver<Command>) -> Result<()> {
    let mut formats = HashMap::<Atom, Format>::new();
    let mut transfers = Vec::<Transfer>::new();
    let timestamp = atom(&reader.connection, "TIMESTAMP")?;
    loop {
        while let Ok(command) = commands.try_recv() {
            let result = (|| -> Result<Stamp> {
                // Compare ownership and publish atomically relative to other clients.
                reader.connection.grab_server()?.check()?;
                let update = (|| -> Result<Stamp> {
                    let current = reader.stamp()?;
                    if command.expected.is_some_and(|expected| expected != current) {
                        bail!("The clipboard changed; restoration cancelled");
                    }
                    formats = command
                        .formats
                        .into_iter()
                        .map(|value| (value.target, value))
                        .collect();
                    reader
                        .connection
                        .set_selection_owner(reader.window, reader.clipboard, CURRENT_TIME)?
                        .check()?;
                    reader.stamp()
                })();
                reader.connection.ungrab_server()?.check()?;
                update
            })();
            let _ = command
                .reply
                .send(result.map_err(|error| error.to_string()));
        }
        while let Some(event) = match reader.pending.pop_front() {
            Some(event) => Some(event),
            None => reader.connection.poll_for_event()?,
        } {
            reader.note(&event);
            match event {
                Event::SelectionRequest(request) if request.selection == reader.clipboard => {
                    let property = if request.property == NONE {
                        request.target
                    } else {
                        request.property
                    };
                    let result = (|| -> Result<()> {
                        if request.target == reader.targets {
                            let targets: Vec<u32> = formats
                                .keys()
                                .copied()
                                .chain([reader.targets, timestamp])
                                .collect();
                            reader
                                .connection
                                .change_property32(
                                    PropMode::REPLACE,
                                    request.requestor,
                                    property,
                                    AtomEnum::ATOM,
                                    &targets,
                                )?
                                .check()?;
                        } else if request.target == timestamp {
                            reader
                                .connection
                                .change_property32(
                                    PropMode::REPLACE,
                                    request.requestor,
                                    property,
                                    AtomEnum::INTEGER,
                                    &[reader.stamp.timestamp],
                                )?
                                .check()?;
                        } else {
                            let value = formats
                                .get(&request.target)
                                .context("Unsupported clipboard format")?;
                            if value.data.len() > CHUNK {
                                if transfers.len() >= 16 {
                                    bail!("Too many clipboard transfers");
                                }
                                reader
                                    .connection
                                    .change_window_attributes(
                                        request.requestor,
                                        &ChangeWindowAttributesAux::new()
                                            .event_mask(EventMask::PROPERTY_CHANGE),
                                    )?
                                    .check()?;
                                reader
                                    .connection
                                    .change_property32(
                                        PropMode::REPLACE,
                                        request.requestor,
                                        property,
                                        reader.incr,
                                        &[value.data.len() as u32],
                                    )?
                                    .check()?;
                                transfers.push(Transfer {
                                    requestor: request.requestor,
                                    property,
                                    value: value.clone(),
                                    offset: 0,
                                    deadline: Instant::now() + Duration::from_secs(10),
                                });
                            } else {
                                write_format(
                                    &reader.connection,
                                    request.requestor,
                                    property,
                                    value,
                                    &value.data,
                                )?;
                            }
                        }
                        Ok(())
                    })();
                    let notify = SelectionNotifyEvent {
                        response_type: xproto::SELECTION_NOTIFY_EVENT,
                        sequence: 0,
                        time: request.time,
                        requestor: request.requestor,
                        selection: request.selection,
                        target: request.target,
                        property: if result.is_ok() { property } else { NONE },
                    };
                    // A requestor can disappear while we respond; it must not stop the service.
                    let _ = reader
                        .connection
                        .send_event(false, request.requestor, EventMask::NO_EVENT, notify)?
                        .check();
                }
                Event::PropertyNotify(event) if event.state == Property::DELETE => {
                    transfers.retain_mut(|transfer| {
                        if transfer.requestor != event.window || transfer.property != event.atom {
                            return true;
                        }
                        let end = (transfer.offset + CHUNK).min(transfer.value.data.len());
                        if write_format(
                            &reader.connection,
                            transfer.requestor,
                            transfer.property,
                            &transfer.value,
                            &transfer.value.data[transfer.offset..end],
                        )
                        .is_err()
                        {
                            return false;
                        }
                        let finished = transfer.offset == end;
                        transfer.offset = end;
                        transfer.deadline = Instant::now() + Duration::from_secs(10);
                        !finished
                    });
                }
                _ => {}
            }
        }
        transfers.retain(|transfer| transfer.deadline > Instant::now());
        reader.connection.flush()?;
        thread::sleep(Duration::from_millis(5));
    }
}

fn write_format(
    connection: &RustConnection,
    window: Window,
    property: Atom,
    value: &Format,
    bytes: &[u8],
) -> Result<()> {
    connection
        .change_property(
            PropMode::REPLACE,
            window,
            property,
            value.type_,
            value.format,
            (bytes.len() / (value.format as usize / 8)) as u32,
            bytes,
        )?
        .check()?;
    Ok(())
}

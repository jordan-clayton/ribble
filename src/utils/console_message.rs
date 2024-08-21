use std::fmt::Formatter;

use strum::Display;

#[derive(Clone, Debug)]
pub struct ConsoleMessage {
    msg_type: ConsoleMessageType,
    msg: String,
}

impl std::fmt::Display for ConsoleMessage {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.msg_type.to_string(), self.msg)
    }
}

impl ConsoleMessage {
    pub fn new(msg_type: ConsoleMessageType, msg: String) -> Self {
        Self { msg_type, msg }
    }
}

#[derive(Clone, Debug, Display)]
pub enum ConsoleMessageType {
    Error,
    Status,
}

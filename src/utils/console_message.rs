use std::fmt::Formatter;

use strum::Display;

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

#[derive(Display)]
pub enum ConsoleMessageType {
    ERROR,
    STATUS,
}
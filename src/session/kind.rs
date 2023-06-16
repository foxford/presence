use std::fmt::{Display, Formatter};

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum SessionKind {
    New,
    Replaced,
}

impl Display for SessionKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let kind = match self {
            SessionKind::New => "new",
            SessionKind::Replaced => "replaced",
        };

        write!(f, "{}", kind)
    }
}

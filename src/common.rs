#[derive(Eq, Clone, Hash, PartialEq, Debug)]
pub enum Actions {
    Start,
    Stop,
    Command,
    Download,
}

impl Actions {
    pub fn code(&self) -> i32 {
        match self {
            Self::Start => 0,
            Self::Stop => 1,
            Self::Command => 2,
            Self::Download => 3,
        }
    }
}

use std::time::Duration;

#[derive(Clone, Debug)]
pub struct PreprocessedFrame {
    pub width: i32,
    pub height: i32,
    pub data: Vec<u8>,
    pub delay: Duration,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PetState {
    Idle,
    Move,
    Drag,
    Clingy,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BehaviorMode {
    Quiet,
    Active,
    Clingy,
}

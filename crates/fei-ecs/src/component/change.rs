#[derive(Default, Copy, Clone)]
pub struct ChangeMark {
    tick: u32,
}

impl ChangeMark {
    #[inline]
    pub fn newer_than(self, other: Self) -> bool {
        // TODO doesn't deal with integer space wraparound.
        self.tick > other.tick
    }
}

#[derive(Default, Copy, Clone)]
pub struct ChangeMarks {
    added: ChangeMark,
    updated: ChangeMark,
}

pub trait ChangeAware {

}

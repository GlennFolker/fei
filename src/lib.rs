pub use fei_common as common;
pub use fei_ecs as ecs;

pub mod prelude {
    pub use super::{
        common::prelude::*,
        ecs::prelude::*,
    };
}

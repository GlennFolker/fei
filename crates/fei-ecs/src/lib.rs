pub use fei_ecs_macros;

pub mod entity;
pub mod component;
pub mod resource;
pub mod system;
pub mod world;

mod change;

pub use change::*;

pub mod prelude {
    pub use fei_ecs_macros::{
        self,
        Component, ComponentSet,
    };
}

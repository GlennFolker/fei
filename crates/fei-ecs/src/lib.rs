pub use fei_ecs_macros;

pub mod entity;
pub mod component;
pub mod resource;
pub mod world;

pub mod prelude {
    pub use fei_ecs_macros::{
        self,
        Component, ComponentSet,
    };
}

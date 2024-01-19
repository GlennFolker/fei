use crate::world::World;

pub trait System: 'static + Send + Sync {
    #[inline]
    fn call(&mut self, world: &mut World) {
        unsafe { self.call_unchecked(world) };
    }

    unsafe fn call_unchecked(&mut self, world: &World);
}

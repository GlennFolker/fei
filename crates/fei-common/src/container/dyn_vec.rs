use std::{
    alloc::{
        self,
        Layout,
    },
    mem,
    ptr::NonNull,
};

/// An unsafe non-statically-known homogenous list data container, similar to [`Vec`](Vec). All the
/// stored elements must outlive the `DynVec` itself.
pub struct DynVec {
    item_layout: Layout,
    array_layout: Layout,
    padded_size: usize,

    array: NonNull<u8>,
    capacity: usize,
    len: usize,
    drop: Option<unsafe fn(*mut u8)>,
}

impl DynVec {
    const fn array_layout(item_layout: Layout, len: usize) -> (Layout, usize) {
        let size = item_layout.size();
        let align = item_layout.align();

        let padded_size = size + (size.wrapping_add(align).wrapping_sub(1) & !align.wrapping_sub(1)).wrapping_sub(size);
        if len == 0 {
            // Safety: 0 is always a valid size.
            return (unsafe { Layout::from_size_align_unchecked(0, align) }, padded_size);
        }

        let Some(alloc_size) = padded_size.checked_mul(len) else {
            panic!("too big allocation size");
        };

        let layout = {
            assert!(alloc_size <= isize::MAX as usize - (align - 1));
            // Safety: Requirements just checked above.
            unsafe { Layout::from_size_align_unchecked(alloc_size, align) }
        };

        (layout, padded_size)
    }

    pub const fn new(item_layout: Layout, drop: Option<unsafe fn(*mut u8)>) -> Self {
        let (array_layout, padded_size) = Self::array_layout(item_layout, 0);
        Self {
            item_layout,
            array_layout,
            padded_size,

            array: NonNull::dangling(),
            capacity: 0,
            len: 0,
            drop,
        }
    }

    pub const fn typed<T>() -> Self {
        #[inline]
        unsafe fn dropper<T>(data: *mut u8) {
            data.cast::<T>().drop_in_place()
        }

        Self::new(Layout::new::<T>(), if mem::needs_drop::<T>() {
            Some(dropper::<T>)
        } else {
            None
        })
    }

    pub unsafe fn with_capacity(capacity: usize, item_layout: Layout, drop: Option<unsafe fn(*mut u8)>) -> Self {
        if capacity == 0 {
            return Self::new(item_layout, drop);
        }

        let (array_layout, padded_size) = Self::array_layout(item_layout, capacity);

        // Safety: Zero-size check has been done.
        let array = alloc::alloc(array_layout);
        if array.is_null() {
            alloc::handle_alloc_error(array_layout);
        }

        Self {
            item_layout,
            array_layout,
            padded_size,

            // Safety: Null-check has been done.
            array: NonNull::new_unchecked(array),
            capacity,
            len: 0,
            drop,
        }
    }
}

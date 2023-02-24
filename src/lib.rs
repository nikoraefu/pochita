#![doc = include_str!("../README.md")]
#![allow(clippy::mut_from_ref, unstable_name_collisions)]

use std::cell::{Cell, RefCell};
use std::mem::MaybeUninit;
use std::ptr::NonNull;

use sptr::Strict;

const PAGE: usize = 4096;
const HUGE_PAGE: usize = 2 * 1024 * 1024;

pub struct DroplessArena<T> {
    start: Cell<*mut T>,
    end: Cell<*mut T>,
    chunks: RefCell<Vec<Chunk<T>>>,
}

impl<T> Default for DroplessArena<T> {
    fn default() -> DroplessArena<T> {
        DroplessArena::new()
    }
}

impl<T> DroplessArena<T> {
    unsafe fn alloc_raw_slice(&self, len: usize) -> *mut T {
        self.ensure_capacity(len);

        let dst = self.start.get();
        self.start.set(dst.add(len));

        dst
    }
}

impl<T> DroplessArena<T> {
    /// Creates a new, empty arena that can be used to allocate objects of type `T`.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena: DroplessArena<i32> = DroplessArena::new();
    /// ```
    pub fn new() -> DroplessArena<T> {
        assert!(std::mem::size_of::<T>() != 0);

        DroplessArena {
            start: Cell::new(std::ptr::null_mut()),
            end: Cell::new(std::ptr::null_mut()),
            chunks: Vec::new().into(),
        }
    }

    /// Determines whether the arena has enough free space to allocate an object of
    /// type `T` with the specified additional size, in bytes.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::<i32>::new();
    /// assert_eq!(arena.can_allocate(10), false);
    /// arena.ensure_capacity(10);
    /// assert_eq!(arena.can_allocate(10), true);
    /// ```
    pub fn can_allocate(&self, additional: usize) -> bool {
        let available_bytes = self.end.get().addr() - self.start.get().addr();
        let additional_bytes = additional.checked_mul(std::mem::size_of::<T>()).unwrap();
        available_bytes >= additional_bytes
    }

    /// Ensures that the arena has enough free space to allocate an object of type
    /// `T` with the specified additional size, in bytes.
    ///
    /// If the arena does not have enough free space, this method will reserve
    /// additional space in the arena to meet the allocation requirements.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::<i32>::new();
    /// assert_eq!(arena.can_allocate(10), false);
    /// arena.ensure_capacity(10);
    /// assert_eq!(arena.can_allocate(10), true);
    /// ```
    pub fn ensure_capacity(&self, additional: usize) {
        if !self.can_allocate(additional) {
            self.reserve(additional);
            debug_assert!(self.can_allocate(additional));
        }
    }

    /// Allocates a new object of type `T` in the arena and initializes it with the
    /// value of the `src` argument.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::new();
    /// let x = arena.alloc(42);
    ///
    /// assert_eq!(*x, 42);
    /// ```
    pub fn alloc(&self, src: T) -> &mut T {
        if self.start == self.end {
            self.reserve(1);
        }

        unsafe {
            let dst = self.start.get();
            self.start.set(self.start.get().add(1));
            dst.write(src);
            &mut *dst
        }
    }

    /// Reserves additional space in the arena to meet the allocation requirements
    /// of an object of type `T` with the specified additional size, in bytes.
    ///
    /// This method will allocate a new chunk of memory to store objects if the
    /// arena is full, and update the arena's start and end pointers to reflect
    /// the new chunk.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::<i32>::new();
    /// arena.reserve(10);
    /// ```
    #[cold]
    #[inline(never)]
    pub fn reserve(&self, additional: usize) {
        let mut chunks = self.chunks.borrow_mut();

        let size = std::mem::size_of::<T>();
        let capacity = match chunks.last_mut() {
            Some(chunk) => chunk.len().min(HUGE_PAGE / size / 2) * 2,
            None => PAGE / size,
        };

        let mut chunk = unsafe { Chunk::new(additional.max(capacity)) };
        self.start.set(chunk.start());
        self.end.set(chunk.end());
        chunks.push(chunk);
    }
}

impl<T: Copy> DroplessArena<T> {
    /// Allocates a new slice of type `T` in the arena and initializes it with a
    /// copy of the contents of the `src` slice passed as an argument.
    ///
    /// # Example
    ///
    /// ```rust
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::new();
    /// let src = [1, 2, 3];
    /// let dst = arena.alloc_slice_copy(&src);
    ///
    /// assert_eq!(src, dst);
    /// ```
    pub fn alloc_slice_copy(&self, src: &[T]) -> &mut [T] {
        let len = src.len();

        if len == 0 {
            return &mut [];
        }

        unsafe {
            let dst = self.alloc_raw_slice(len);
            src.as_ptr().copy_to_nonoverlapping(dst, len);
            std::slice::from_raw_parts_mut(dst, len)
        }
    }
}

impl<T: Clone> DroplessArena<T> {
    /// Allocates a slice of objects of type `T` in this arena and initializes
    /// them with a clone of the values in the provided slice.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::new();
    /// let slice = arena.alloc_slice_clone(&[1, 2, 3]);
    /// assert_eq!(slice, &[1, 2, 3]);
    /// ```
    pub fn alloc_slice_clone(&self, src: &[T]) -> &mut [T] {
        let len = src.len();

        if len == 0 {
            return &mut [];
        }

        unsafe {
            let dst = self.alloc_raw_slice(len);
            for (index, item) in src.iter().cloned().enumerate() {
                dst.add(index).write(item);
            }
            std::slice::from_raw_parts_mut(dst, len)
        }
    }
}

impl DroplessArena<u8> {
    /// Allocates a new string in the arena and initializes it with a copy of the
    /// contents of the `src` string passed as an argument.
    ///
    /// # Example
    ///
    /// ```
    /// use pochita::DroplessArena;
    ///
    /// let arena = DroplessArena::new();
    /// let src = "hello, world!";
    /// let dst = arena.alloc_str(src);
    ///
    /// assert_eq!(src, dst);
    /// ```
    pub fn alloc_str(&self, src: &str) -> &mut str {
        let bytes = self.alloc_slice_copy(src.as_bytes());
        unsafe { std::str::from_utf8_unchecked_mut(bytes) }
    }
}

struct Chunk<T> {
    storage: NonNull<[MaybeUninit<T>]>,
}

impl<T> Chunk<T> {
    unsafe fn new(capacity: usize) -> Chunk<T> {
        // TODO: replace with `Box::new_uninit_slice` once https://github.com/rust-lang/rust/issues/63291 is stabilized.
        let uninit_slice = {
            let mut uninit_slice = Vec::with_capacity(capacity);
            uninit_slice.set_len(capacity);
            uninit_slice.into_boxed_slice()
        };
        Chunk { storage: NonNull::new_unchecked(Box::into_raw(uninit_slice)) }
    }

    fn len(&self) -> usize {
        unsafe { (*self.storage.as_ptr()).len() }
    }

    fn start(&mut self) -> *mut T {
        self.storage.as_ptr() as *mut T
    }

    fn end(&mut self) -> *mut T {
        unsafe { self.start().add((*self.storage.as_ptr()).len()) }
    }
}

impl<T> Drop for Chunk<T> {
    fn drop(&mut self) {
        unsafe { Box::from_raw(self.storage.as_mut()) };
    }
}

#[cfg(test)]
mod tests {
    use crate::DroplessArena;

    #[test]
    fn alloc() {
        let arena = DroplessArena::new();

        assert_eq!(*arena.alloc("Pochita"), "Pochita");
    }

    #[test]
    fn alloc_slice_copy() {
        let arena = DroplessArena::new();

        assert_eq!(arena.alloc_slice_copy(b"Pochita"), b"Pochita");
    }

    #[test]
    fn alloc_slice_clone() {
        #[derive(Clone, Debug, PartialEq, Eq)]
        struct NonCopy(&'static str);

        let arena = DroplessArena::new();
        assert_eq!(
            arena.alloc_slice_clone(&[NonCopy("Pochita"), NonCopy("Makima")]),
            &[NonCopy("Pochita"), NonCopy("Makima")]
        );
    }

    #[test]
    fn alloc_slice_str() {
        let arena = DroplessArena::new();

        assert_eq!(arena.alloc_str("Makima"), "Makima");
        assert_eq!(arena.alloc_str("Pochita"), "Pochita");
    }
}

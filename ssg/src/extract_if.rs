//! Code for `alloc::vec::Vec::extract_if()` and `alloc::vec::ExtractIf`.
//! These are currently unstable, so they have been ported for use in stable Rust.
//! https://github.com/rust-lang/rust/issues/43244

use std::{ptr, slice};

/// Creates an iterator which uses a closure to determine if an element should be removed.
///
/// If the closure returns true, then the element is removed and yielded.
/// If the closure returns false, the element remains in the vector and is not yielded.
///
/// If the returned `ExtractIf` is not exhausted, e.g. because it is dropped without iterating
/// or the iteration short-circuits, then the remaining elements will be retained.
/// Use `Vec::retain()` (or `Vec::retain_mut()` if you want mutable references to elements)
/// with a negated predicate if you do not need the returned iterator.
pub(crate) fn extract_if<T, F>(vec: &mut Vec<T>, filter: F) -> ExtractIf<'_, T, F>
where
    F: FnMut(&mut T) -> bool,
{
    let old_len = vec.len();

    // Guard against us getting leaked (leak amplification)
    unsafe {
        vec.set_len(0);
    }

    ExtractIf {
        vec,
        idx: 0,
        del: 0,
        old_len,
        pred: filter,
    }
}

/// An iterator which uses a closure to determine if an element should be removed.
///
/// This struct is created by `extract_if()`. See its documentation for more information.
#[derive(Debug)]
#[must_use = "iterators are lazy and do nothing unless consumed"]
pub(crate) struct ExtractIf<'a, T, F>
where
    F: FnMut(&mut T) -> bool,
{
    vec: &'a mut Vec<T>,
    /// The index of the item that will be inspected by the next call to `next`.
    idx: usize,
    /// The number of items that have been drained (removed) thus far.
    del: usize,
    /// The original length of `vec` prior to draining.
    old_len: usize,
    /// The filter test predicate.
    pred: F,
}

impl<T, F> Iterator for ExtractIf<'_, T, F>
where
    F: FnMut(&mut T) -> bool,
{
    type Item = T;

    fn next(&mut self) -> Option<T> {
        unsafe {
            while self.idx < self.old_len {
                let i = self.idx;
                let v = slice::from_raw_parts_mut(self.vec.as_mut_ptr(), self.old_len);
                let drained = (self.pred)(&mut v[i]);
                // Update the index *after* the predicate is called. If the index
                // is updated prior and the predicate panics, the element at this
                // index would be leaked.
                self.idx += 1;
                if drained {
                    self.del += 1;
                    return Some(ptr::read(&v[i]));
                } else if self.del > 0 {
                    let del = self.del;
                    let src: *const T = &v[i];
                    let dst: *mut T = &mut v[i - del];
                    ptr::copy_nonoverlapping(src, dst, 1);
                }
            }
            None
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (0, Some(self.old_len - self.idx))
    }
}

impl<T, F> Drop for ExtractIf<'_, T, F>
where
    F: FnMut(&mut T) -> bool,
{
    fn drop(&mut self) {
        unsafe {
            if self.idx < self.old_len && self.del > 0 {
                // This is a pretty messed up state, and there isn't really an
                // obviously right thing to do. We don't want to keep trying
                // to execute `pred`, so we just backshift all the unprocessed
                // elements and tell the vec that they still exist. The backshift
                // is required to prevent a double-drop of the last successfully
                // drained item prior to a panic in the predicate.
                let ptr = self.vec.as_mut_ptr();
                let src = ptr.add(self.idx);
                let dst = src.sub(self.del);
                let tail_len = self.old_len - self.idx;
                src.copy_to(dst, tail_len);
            }
            self.vec.set_len(self.old_len - self.del);
        }
    }
}

use std::cell::UnsafeCell;
use std::collections::LinkedList;
use std::mem::MaybeUninit;
use std::sync::{
    atomic::{AtomicU32, Ordering},
    Arc, RwLock,
};

const CHUNK_SIZE: usize = 128;

/// StableList provides a List type that allows for an arbitrary number of simultaneous lockless
/// readers and with a single locking writer. Readers are never interrupted by a writer.
///
/// In order to provide this guarantee, the list will never delete an item or move its location in
/// memory. Items can only be deleted by dropping all copies of the list.
#[derive(Clone)]
pub struct StableList<T>(Arc<StableListInner<T>>);

impl<T> StableList<T> {
    pub fn new() -> Self {
        Self(Arc::new(StableListInner::new()))
    }

    /// Provide an iterator for the entire list.
    ///
    /// Iterator is created and operates via lockless operations.
    pub fn iter(&self) -> StableListIterator<T> {
        StableListIterator {
            global_idx: 0,
            chunk: std::ptr::null(),
            list: self,
        }
    }

    //
    // passthrough functions
    //

    /// Push new item onto back of list.
    pub fn push(&self, item: T) {
        self.0.push(item)
    }

    /// Get single item from list.
    ///
    /// This will acquire a lock, for lockless reading use the `iter` function.
    pub fn get(&self, idx: usize) -> Option<&T> {
        self.0.get(idx)
    }

    /// Returns current length of the list.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Returns an internal chunk
    ///
    /// # Safety
    ///
    /// Caller is responsible for ensuring that any elements accessed in chunk have been
    /// initialized. Any element before the current len is considered valid.
    pub unsafe fn get_chunk(
        &self,
        idx: usize,
    ) -> Option<*const [UnsafeCell<MaybeUninit<T>>; CHUNK_SIZE]> {
        self.0.get_chunk(idx)
    }
}

struct StableListInner<T> {
    list_lock: RwLock<LinkedList<*const [UnsafeCell<MaybeUninit<T>>; CHUNK_SIZE]>>,

    /// Index just past the last initialized item in the StableList
    ///
    /// The item pointed to by this idx is uninitialized or may not exist.
    last_global_idx: AtomicU32,
}

// TODO Document
unsafe impl<T> Send for StableListInner<T> where T: Send {}
unsafe impl<T> Sync for StableListInner<T> {}

impl<T> StableListInner<T> {
    fn new() -> Self {
        let list: LinkedList<*const _> = LinkedList::new();
        StableListInner {
            list_lock: RwLock::new(list),
            last_global_idx: AtomicU32::new(0),
        }
    }

    fn push(&self, item: T) {
        let mut list = match self.list_lock.write() {
            Ok(lock) => lock,
            Err(_) => panic!("StableList's internal mutex has been poisoned"),
        };
        // make sure to get the most recent value, don't move this before the lock
        let global_idx = self.last_global_idx.load(Ordering::SeqCst) as usize;
        if global_idx == u32::MAX as usize {
            panic!("list is full, cannot index past 2^32");
        }
        if global_idx % CHUNK_SIZE == 0 {
            // we have all full blocks and have to add a new one
            // Safety: We are telling the compiler to assume initialization of the MaybeUninit values
            // *not* the T inside them. MaybeUninit requires no initialization.
            #[allow(clippy::uninit_assumed_init)]
            let block: [UnsafeCell<MaybeUninit<T>>; CHUNK_SIZE] =
                unsafe { MaybeUninit::uninit().assume_init() };
            list.push_back(Box::into_raw(Box::new(block)));
        }
        let last_block = list
            .iter_mut()
            .last()
            .expect("no block in list even though we tried to add one");
        // Safety: value pointed to by global_idx has not yet been initialized but it safe to write
        // to uninitialized memory. And it is not visible to anyone obeying promises of
        // `get_chunk`, so it is safe to write to it with this exclusive access.
        unsafe { *(**last_block)[global_idx % CHUNK_SIZE].get() = MaybeUninit::new(item) };
        // Safety: only modify last_global_idx while we have the lock
        self.last_global_idx.fetch_add(1, Ordering::SeqCst);
    }

    /// Returns list length
    fn len(&self) -> usize {
        self.last_global_idx.load(Ordering::SeqCst) as usize
    }

    unsafe fn get_chunk(
        &self,
        idx: usize,
    ) -> Option<*const [UnsafeCell<MaybeUninit<T>>; CHUNK_SIZE]> {
        match self.list_lock.read() {
            Ok(lock) => lock.iter().nth(idx).copied(),
            Err(_) => panic!("StableList's internal mutex has been poisoned"),
        }
    }

    fn get(&self, idx: usize) -> Option<&T> {
        if idx < self.last_global_idx.load(Ordering::SeqCst) as usize {
            let list = match self.list_lock.read() {
                Ok(lock) => lock,
                Err(_) => panic!("StableList's internal mutex has been poisoned"),
            };
            // Safety: All values before last_global_idx are guaranteed to be initialized
            list.iter()
                .nth(idx / CHUNK_SIZE)
                .map(|ch| unsafe { unwrap_value(&(&**ch)[idx % CHUNK_SIZE]) })
        } else {
            None
        }
    }
}

// Call to convert a value wrapped in UnsafeCell<MaybeUninit<T>> to T
//
// # Safety
// Caller must guarantee that the location pointed to by cell is initialized.
// Caller must also guarantee that value will not be modified while this reference is alive.
//
// Failure to provide the above guarantees will result in Undefined Behavior.
unsafe fn unwrap_value<'a, T>(cell: &'a UnsafeCell<MaybeUninit<T>>) -> &'a T {
    &*cell.get().as_ref().unwrap().as_ptr().as_ref().unwrap()
}

// TODO impl Drop for StableList, by default dropping MaybeUninit does nothing resulting in the
// internal values leaking if they are heap allocated

pub struct StableListIterator<'a, T> {
    // TODO rename to not be global
    global_idx: usize,
    chunk: *const [UnsafeCell<MaybeUninit<T>>; CHUNK_SIZE],
    list: &'a StableList<T>,
}

impl<'a, T> Iterator for StableListIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.chunk.is_null() {
            match unsafe { self.list.0.get_chunk(0) } {
                Some(next_chunk) => self.chunk = next_chunk,
                None => return None,
            }
            // TODO simplify all this and move it into a function for thorough documentation
            return Some(unsafe { unwrap_value(&(&*self.chunk)[self.global_idx % CHUNK_SIZE]) });
        }
        if self.global_idx + 1 == self.list.len() {
            // no values to return right now
            return None;
        }

        if self.global_idx % CHUNK_SIZE + 1 == CHUNK_SIZE {
            // this would be a lot simpler if LinkedList exposed a way to hold a reference to a
            // node, the proposed cursor API might be what is necessary: https://github.com/rust-lang/rust/issues/58533
            // TODO safety
            match unsafe { self.list.0.get_chunk(self.global_idx / CHUNK_SIZE + 1) } {
                None => return None,
                Some(chunk) => {
                    self.global_idx += 1;
                    self.chunk = chunk;
                }
            }
        } else {
            self.global_idx += 1;
        }
        return Some(unsafe { unwrap_value(&(&*self.chunk)[self.global_idx % CHUNK_SIZE]) });
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    // Test that an item pushed into StableList can then be retrieved via `get` and the iterator
    fn push_and_check_single_item() {
        let list = StableList::new();
        assert_eq!(list.get(0), None);
        list.push(1002);
        assert_eq!(list.get(0), Some(&1002));
        assert_eq!(list.iter().next(), Some(&1002));
    }

    #[test]
    fn push_and_check_full_chunk() {
        let list = StableList::new();
        assert_eq!(list.get(0), None);
        for i in 0..CHUNK_SIZE {
            list.push(100 + i);
        }
        for i in 0..CHUNK_SIZE {
            assert_eq!(list.get(i), Some(&(100 + i)));
        }
    }

    #[test]
    fn push_and_check_multiple_chunks() {
        let list = StableList::new();
        assert_eq!(list.get(0), None);
        for i in 0..(CHUNK_SIZE * 2) {
            list.push(100 + i);
        }
        for i in 0..(CHUNK_SIZE * 2) {
            assert_eq!(list.get(i), Some(&(100 + i)));
        }
    }

    #[test]
    // Test that populating the list then iterating over it works
    fn populate_and_iterate_simple() {
        let list = StableList::new();
        let iter = list.iter();
        let arb_values = CHUNK_SIZE * 2 + 1;
        for i in 0..(arb_values) {
            list.push(i * 10);
        }
        assert_eq!(list.len(), arb_values);
        let mut values_found = 0;
        for (exp, val) in (0..).zip(iter) {
            assert_eq!(exp * 10, *val);
            values_found += 1;
        }
        assert_eq!(values_found, arb_values);
    }

    #[test]
    // Test that iterator will return values again after returning None if new values are added to
    // base list
    fn iterator_resumption() {
        let list = StableList::new();
        let mut iter = list.iter();
        assert_eq!(list.len(), 0);
        assert_eq!(iter.next(), None);
        list.push(1000);
        assert_eq!(list.len(), 1);
        assert_eq!(iter.next(), Some(&1000));
    }

    #[test]
    // Test that handing out multiple iterators at the same time works.
    fn multiple_iterators() {
        let list = StableList::<i32>::new();
        let mut iter_1 = list.iter();
        let mut iter_2 = list.iter();
        for i in 100..200 {
            list.push(i);
            let a = iter_1.next();
            let b = iter_2.next();
            assert_eq!(Some(&i), a);
            assert_eq!(Some(&i), b);
        }
    }
}

use std::collections::LinkedList;
use std::mem::MaybeUninit;

const BLOCK_SIZE: usize = 128;

pub struct StableList<T> {
    // TODO do I need to use a LinkedList, can I use a Vec and only protect access to it with all
    // iterators caching their current array?
    list: LinkedList<Box<[MaybeUninit<T>; BLOCK_SIZE]>>,
    /// Index just past the last initialized item in the StableList
    ///
    /// The item pointed to by this idx is uninitialized or may not exist.
    last_global_idx: usize,
}

impl<T> StableList<T> {
    pub fn new() -> Self {
        StableList{
            list: LinkedList::new(),
            last_global_idx: 0,
        }
    }

    pub fn push(&mut self, item: T) {
       if self.last_global_idx % BLOCK_SIZE == 0 {
            // we have all full blocks and have to add a new one
            self.add_block();
       }
       let last_block = self.list.iter_mut().last().expect("no block in list even though we called add_block");
       let idx = self.last_global_idx % BLOCK_SIZE;
       last_block[idx] = MaybeUninit::new(item);
       self.last_global_idx += 1;
    }

    pub fn len(&self) -> usize {
       self.last_global_idx 
    }

    fn add_block(&mut self) {
        // Safety: We are telling the compiler to assume initialization of the MaybeUninit values
        // *not* the T inside them. MaybeUninit requires no initialization.
        let block: [MaybeUninit<T>; BLOCK_SIZE] = unsafe { MaybeUninit::uninit().assume_init() };
        self.list.push_back(Box::new(block));
    }

    fn get(&self, idx: usize) -> Option<&T> {
        if idx < self.last_global_idx {
            // Safety: All values less before last_global_idx are guaranteed to be initialized
            self.list.iter().take(idx / BLOCK_SIZE + 1).last().map(|ch| unsafe { &*ch[idx % BLOCK_SIZE].as_ptr() })
        } else {
            None
        }
    }

    fn iter(&self) -> StableListIterator<'_, T> {
        StableListIterator {
            global_idx: self.last_global_idx,
            chunk_idx: self.last_global_idx % BLOCK_SIZE,
            chunk: self.list.iter().last(),
            list: self,
        }
    }
}

// TODO impl Drop for StableList, by default dropping MaybeUninit does nothing resulting in the
// internal values leaking if they are heap allocated

struct StableListIterator<'a, T: 'a> {
    global_idx: usize,
    chunk_idx: usize,
    chunk: Option<&'a Box<[MaybeUninit<T>; BLOCK_SIZE]>>,
    list: &'a StableList<T>,
}

impl<'a, T> Iterator for StableListIterator<'a, T> {
    type Item = &'a T;

    fn next(&mut self) -> Option<Self::Item> {
        if self.global_idx == self.list.last_global_idx {
            // no more values to return
            return None;
        }

        if self.chunk_idx + 1 == BLOCK_SIZE {
            // this would be a lot simpler if LinkedList exposed a way to hold a reference to a
            // node, the proposed cursor API might be what is necessary: https://github.com/rust-lang/rust/issues/58533
            match self.list.list.iter().take(self.global_idx % BLOCK_SIZE + 1).last() {
                None => return None,
                Some(chunk) => {
                    self.chunk_idx = 0;
                    self.global_idx += 1;
                    self.chunk = Some(chunk);
                    return self.chunk.map(|ch| unsafe { &*ch[self.chunk_idx].as_ptr()} )
                },
            }
        }
        self.chunk_idx += 1;
        self.global_idx += 1;
        return self.chunk.map(|ch| unsafe { &*ch[self.chunk_idx].as_ptr()} )
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    #[test]
    // Test that an item pushed into StableList can then be retrieved via `get`
    fn push_and_check_single_item() {
        let mut list = StableList::new();
        assert_eq!(list.get(0), None);
        list.push(0xABABi32);
        assert_eq!(list.get(0), Some(&0xABAB));
    }

    #[test]
    // Test that populating the list then iterating over it works
    fn populate_and_iterate_simple() {
        let mut list = StableList::new();
        let mut iter = list.iter();
        for i in 0..(BLOCK_SIZE*5) {
            list.push(i);
        }
        let mut values = 0;
        for (exp, val) in (0..).zip(list.iter()) {
            assert_eq!(exp, *val);
            values += 1;
        }
        assert_eq!(values, BLOCK_SIZE*5);
    }

    #[test]
    // Test that iterator will return values again after returning None if new values are added to
    // base list
    fn iterator_resumption() {
        let mut list = StableList::new();
        let mut iter = list.iter();
        assert_eq!(iter.next(), None);
        list.push(0xBEEF);
        assert_eq!(iter.next(), Some(&0xBEEF));
    }

    //proptest! {
    //    #[test]
    //    fn test_calc_local_idx(last_global: usize, block_size: std::num::NonZeroUsize) {
    //        let idx = StableList::calc_local_idx(last_global, block_size);
    //        assert!(idx < block_size);
    //        assert!(idx < block_size);
    //        assert!(idx == last_global % block_size);
    //    }
    //}
}

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
    last_global_idx: Atomic<usize>,
}

impl<T> StableList<T> {
    pub fn new() -> Self {
        StableList{
            list: LinkedList::new(),
            last_global_idx: 0,
        }
    }

    pub fn push(&mut self, item: T) {
       if BLOCK_SIZE % self.last_global_idx == 0 {
            // we have all full blocks and have to add a new one
            self.add_block();
       }
       let mut last_block = self.list.iter().last().expect("no block in list even though we called add_block");
       let idx = Self::calc_local_idx(self.last_global_idx, BLOCK_SIZE);
       last_block[idx] = MaybeUninit::new(item);
       self.last_global_idx += 1;
    }

    pub fn len(&self) -> usize {
       self.last_global_idx 
    }

    fn add_block(&mut self) {
        self.list.push_back(Box::new([MaybeUninit::uninit(); BLOCK_SIZE]));
    }

    fn iter(&self) -> impl Iterator {
        StableListIterator {
            global_idx: self.last_global_idx,
            chunk_idx: self.last_global_idx % BLOCK_SIZE,
            chunk: self.list.iter().last(),
            list: self,
        }
    }
}

struct StableListIterator<T> {
    global_idx: usize,
    chunk_idx: usize,
    chunk: Option<Box<[MaybeUninit<T>; BLOCK_SIZE]>>,
    list: StableList<T>,
}

impl Iterator for StableListIterator<T> {
    type Item = T;

    pub fn next(&mut self) -> Option<Self::Item> {
        if self.global_idx == self.list.last_global_idx {
            // no more values to return
            return None;
        }

        if self.chunk_idx + 1 == BLOCK_SIZE {
            // this would be a lot simpler if LinkedList exposed a way to hold a reference to a
            // node, the proposed cursor API might be what is necessary: https://github.com/rust-lang/rust/issues/58533
            match self.list.iter().take(self.global_idx % BLOCK_SIZE + 1) {
                None => return None,
                Some(chunk) => {
                    self.chunk_idx = 0;
                    self.global_idx += 1;
                    self.chunk = Some(chunk);
                    return Some(self.chunk.unwrap()[0])
                },
            }
        }
        self.chunk_idx += 1;
        self.global_idx += 1;
        return self.chunk.map(|ch| ch[self.chunk_idx])
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn test_calc_local_idx(last_global: usize, block_size: std::num::NonZeroUsize) {
            let idx = StableList::calc_local_idx(last_global, block_size);
            assert!(idx < block_size);
            assert!(idx < block_size);
            assert!(idx == last_global % block_size);
        }
    }
}

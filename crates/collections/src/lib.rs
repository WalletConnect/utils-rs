use std::{collections::HashMap, hash::BuildHasher};

/// A trait to provide memory optimization functionality to [`HashMap`].
pub trait HashMapExt {
    /// Attempts to optimize the map's memory consumption by shrinking it if the
    /// number of entries is a lot less than its capacity.
    fn optimize(&mut self);
}

impl<K, V, H> HashMapExt for HashMap<K, V, H>
where
    K: Eq + std::hash::Hash,
    H: BuildHasher,
{
    #[inline]
    fn optimize(&mut self) {
        if self.len() * 3 < self.capacity() {
            self.shrink_to_fit();
        }
    }
}

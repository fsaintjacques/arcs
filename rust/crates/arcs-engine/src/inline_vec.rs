//! A minimal fixed-capacity vector for the bounded lists in game state.
//!
//! Everything in Arcs is statically bounded (players <= 4, systems = 24,
//! decks <= 31, ...), so no list ever needs the heap. `InlineVec` is `Copy`
//! when `T` is, which is what keeps `GameState: Copy` in later milestones.

/// A fixed-capacity vector of at most `N` elements, stored inline.
///
/// Overflow is a bug in the caller: `push` debug-asserts and drops the
/// element in release builds.
#[derive(Clone, Copy)]
pub struct InlineVec<T, const N: usize> {
    len: u8,
    items: [T; N],
}

impl<T: Copy + Default, const N: usize> InlineVec<T, N> {
    #[inline]
    pub fn new() -> Self {
        Self {
            len: 0,
            items: [T::default(); N],
        }
    }

    #[inline]
    pub fn from_slice(items: &[T]) -> Self {
        let mut v = Self::new();
        for &item in items {
            v.push(item);
        }
        v
    }
}

impl<T: Copy, const N: usize> InlineVec<T, N> {
    /// An empty vector whose dead slots hold `fill` (for `T` without
    /// `Default`).
    #[inline]
    pub fn filled(fill: T) -> Self {
        Self {
            len: 0,
            items: [fill; N],
        }
    }

    #[inline]
    pub fn push(&mut self, item: T) {
        debug_assert!((self.len as usize) < N, "InlineVec overflow (cap {N})");
        if (self.len as usize) < N {
            self.items[self.len as usize] = item;
            self.len += 1;
        }
    }

    #[inline]
    pub fn pop(&mut self) -> Option<T> {
        if self.len == 0 {
            return None;
        }
        self.len -= 1;
        Some(self.items[self.len as usize])
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.len as usize
    }

    #[inline]
    pub fn is_full(&self) -> bool {
        self.len as usize == N
    }

    /// Remove and return the element at `index`, shifting the tail left
    /// (the `Vec::remove` / JS `splice(index, 1)` shape).
    pub fn remove(&mut self, index: usize) -> T {
        assert!(index < self.len as usize, "InlineVec::remove out of bounds");
        let item = self.items[index];
        for i in index..self.len as usize - 1 {
            self.items[i] = self.items[i + 1];
        }
        self.len -= 1;
        item
    }

    /// Index of the first element equal to `item`.
    pub fn position(&self, item: &T) -> Option<usize>
    where
        T: PartialEq,
    {
        self.as_slice().iter().position(|x| x == item)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[inline]
    pub fn as_slice(&self) -> &[T] {
        &self.items[..self.len as usize]
    }

    #[inline]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.items[..self.len as usize]
    }

    #[inline]
    pub fn iter(&self) -> core::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    #[inline]
    pub fn clear(&mut self) {
        self.len = 0;
    }

    /// Keep only the elements for which `keep` returns true, preserving
    /// order.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        let mut kept = 0usize;
        for i in 0..self.len as usize {
            if keep(&self.items[i]) {
                self.items[kept] = self.items[i];
                kept += 1;
            }
        }
        self.len = kept as u8;
    }

    pub fn contains(&self, item: &T) -> bool
    where
        T: PartialEq,
    {
        self.as_slice().contains(item)
    }
}

impl<T: Copy + Default, const N: usize> Default for InlineVec<T, N> {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl<T, const N: usize> core::ops::Deref for InlineVec<T, N> {
    type Target = [T];

    #[inline]
    fn deref(&self) -> &[T] {
        &self.items[..self.len as usize]
    }
}

// Compare, hash and print only the live prefix — dead slots are not state.
impl<T: Copy + PartialEq, const N: usize> PartialEq for InlineVec<T, N> {
    fn eq(&self, other: &Self) -> bool {
        self.as_slice() == other.as_slice()
    }
}

impl<T: Copy + Eq, const N: usize> Eq for InlineVec<T, N> {}

impl<T: Copy + core::hash::Hash, const N: usize> core::hash::Hash for InlineVec<T, N> {
    fn hash<H: core::hash::Hasher>(&self, state: &mut H) {
        self.as_slice().hash(state);
    }
}

impl<T: Copy + core::fmt::Debug, const N: usize> core::fmt::Debug for InlineVec<T, N> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_list().entries(self.as_slice()).finish()
    }
}

impl<'a, T: Copy, const N: usize> IntoIterator for &'a InlineVec<T, N> {
    type Item = &'a T;
    type IntoIter = core::slice::Iter<'a, T>;

    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

impl<T: Copy + Default, const N: usize> FromIterator<T> for InlineVec<T, N> {
    fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Self {
        let mut v = Self::new();
        for item in iter {
            v.push(item);
        }
        v
    }
}

#[cfg(feature = "serde")]
impl<T: Copy + serde::Serialize, const N: usize> serde::Serialize for InlineVec<T, N> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_seq(self.as_slice())
    }
}

#[cfg(feature = "serde")]
impl<'de, T, const N: usize> serde::Deserialize<'de> for InlineVec<T, N>
where
    T: Copy + Default + serde::Deserialize<'de>,
{
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let items = Vec::<T>::deserialize(deserializer)?;
        if items.len() > N {
            return Err(serde::de::Error::invalid_length(
                items.len(),
                &"at most the InlineVec capacity",
            ));
        }
        Ok(Self::from_slice(&items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn push_pop_len() {
        let mut v: InlineVec<u8, 4> = InlineVec::new();
        assert!(v.is_empty());
        v.push(1);
        v.push(2);
        v.push(3);
        assert_eq!(v.len(), 3);
        assert_eq!(v.as_slice(), &[1, 2, 3]);
        assert_eq!(v.pop(), Some(3));
        assert_eq!(v.pop(), Some(2));
        assert_eq!(v.pop(), Some(1));
        assert_eq!(v.pop(), None);
    }

    #[test]
    fn retain_preserves_order() {
        let mut v: InlineVec<u8, 8> = InlineVec::from_slice(&[1, 2, 3, 4, 5, 6]);
        v.retain(|x| x % 2 == 0);
        assert_eq!(v.as_slice(), &[2, 4, 6]);
    }

    #[test]
    fn eq_ignores_dead_slots() {
        let mut a: InlineVec<u8, 4> = InlineVec::from_slice(&[1, 2, 3]);
        a.pop();
        let b: InlineVec<u8, 4> = InlineVec::from_slice(&[1, 2]);
        assert_eq!(a, b);
    }

    #[test]
    #[should_panic(expected = "InlineVec overflow")]
    #[cfg(debug_assertions)]
    fn overflow_debug_asserts() {
        let mut v: InlineVec<u8, 2> = InlineVec::new();
        v.push(1);
        v.push(2);
        v.push(3);
    }

    #[test]
    fn filled_supports_non_default_types() {
        #[derive(Clone, Copy, PartialEq, Debug)]
        enum E {
            A,
            B,
        }
        let mut v: InlineVec<E, 3> = InlineVec::filled(E::A);
        v.push(E::B);
        assert_eq!(v.as_slice(), &[E::B]);
    }
}

use std::collections::BTreeSet;

/// A fixed size ordered queue.
#[derive(Clone, Debug)]
pub struct SegmentQueue<S>
where
    S: Ord,
{
    queue: BTreeSet<S>,
    size: usize,
}

impl<S> SegmentQueue<S>
where
    S: Ord,
{
    #[inline]
    pub fn new(size: usize) -> Self {
        Self {
            queue: BTreeSet::new(),
            size,
        }
    }

    #[inline]
    pub fn push(&mut self, segment: S) {
        if self.queue.len() == self.size {
            return;
        }

        self.queue.insert(segment);
    }

    #[inline]
    pub fn peek(&mut self) -> Option<&'_ S> {
        self.queue.first()
    }

    #[inline]
    pub fn pop(&mut self) -> Option<S> {
        self.queue.pop_first()
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.queue.len()
    }

    #[inline]
    pub fn capacity(&self) -> usize {
        self.size
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    #[inline]
    pub fn clear(&mut self) {
        self.queue.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::SegmentQueue;

    #[test]
    fn test_queue() {
        let mut queue = SegmentQueue::new(8192);
        queue.push(5);

        assert_eq!(*queue.peek().unwrap(), 5);

        queue.push(6);
        queue.push(7);
        queue.push(8);
        assert_eq!(*queue.peek().unwrap(), 5);

        assert_eq!(queue.pop().unwrap(), 5);
        assert_eq!(*queue.peek().unwrap(), 6);

        queue.push(4);
        assert_eq!(*queue.peek().unwrap(), 4);

        for val in [4, 6, 7, 8] {
            assert_eq!(*queue.peek().unwrap(), val);
            assert_eq!(queue.pop().unwrap(), val);
        }

        assert_eq!(queue.peek(), None);
        assert_eq!(queue.pop(), None);
        assert_eq!(queue.len(), 0);
        assert!(queue.is_empty());
    }
}

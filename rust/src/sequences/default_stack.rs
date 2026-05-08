#[derive(Clone, Debug)]
pub struct DefaultStack<T> {
    items: Vec<T>,
}

impl<T> Default for DefaultStack<T> {
    fn default() -> Self {
        Self { items: Vec::new() }
    }
}

impl<T> DefaultStack<T> {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, item: T) {
        self.items.push(item);
    }

    pub fn pop(&mut self) -> Option<T> {
        self.items.pop()
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }
}

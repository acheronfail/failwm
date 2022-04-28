use std::fmt::Display;

#[derive(Debug)]
pub enum Node<B, L> {
    Branch { inner: B, children: Vec<Node<B, L>> },
    Leaves { inner: B, children: Vec<Leaf<L>> },
}

impl<B, L> Node<B, L> {
    pub fn leaves(inner: B) -> Self {
        Self::Leaves {
            inner,
            children: vec![],
        }
    }

    pub fn branch(inner: B) -> Self {
        Self::Branch {
            inner,
            children: vec![],
        }
    }
}

#[derive(Debug)]
pub struct Leaf<T>(pub T);

impl<T> Leaf<T> {
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: Display> Display for Leaf<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Leaf({})", self.0)
    }
}
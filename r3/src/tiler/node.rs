use std::fmt::Display;
use std::sync::{Arc, Weak};

// TODO: consider sharing common fields, and using enum only for children

#[derive(Debug)]
pub enum Node<B, L> {
    Branch {
        parent: Weak<Node<B, L>>,
        inner: Option<B>,
        children: Vec<Arc<Node<B, L>>>,
    },
    Leaves {
        parent: Weak<Node<B, L>>,
        inner: Option<B>,
        children: Vec<Leaf<L>>,
    },
}

impl<B, L> Node<B, L> {
    pub fn leaves(inner: Option<B>) -> Self {
        Self::Leaves {
            parent: Weak::new(),
            inner,
            children: vec![],
        }
    }

    pub fn branch(inner: Option<B>) -> Self {
        Self::Branch {
            parent: Weak::new(),
            inner,
            children: vec![],
        }
    }

    pub fn is_root(&self) -> bool {
        (match self {
            Node::Leaves { parent, .. } => parent,
            Node::Branch { parent, .. } => parent,
        })
        .upgrade()
        .is_none()
    }

    // FIXME: need different operations for adding `Leaf` structs, as opposed to `Node`s

    pub fn append_child(&mut self, node: Node<B, L>) {
        match self {
            Node::Branch { children, .. } => {
                children.push(Arc::new(node));
            }
            Node::Leaves { ref parent, .. } => {
                let new_parent = parent.upgrade().map_or_else(
                    || Weak::new(),
                    |n| {
                        match n.as_ref() {
                            Node::Branch { children, .. } => {
                                // SAFETY: TODO
                                let me = children
                                    .iter()
                                    // ???: as *const _ ??? TODO: does this actually work how I think it does?
                                    .find(|c| c.as_ref() as *const _ == self as *const _)
                                    .unwrap();
                                Arc::downgrade(me)
                            }
                            // SAFETY: TODO
                            _ => unreachable!(),
                        }
                    },
                );

                let (children, inner) = if let Node::Leaves { children, inner, .. } = self {
                    (children, inner)
                } else {
                    unreachable!();
                };
                // TODO: doc
                let n = Node::Leaves {
                    // FIXME: needs to be new self !!! need to get parent's Arc and downgrade it?
                    parent: new_parent,
                    inner: inner.take(),
                    children: children.drain(..).collect(),
                };

                // TODO: doc
                *self = Node::Branch {
                    // TODO: doc - if root this is None, if non-root it's parent
                    parent: parent.clone(),
                    // TODO: doc
                    inner: None,
                    // TODO: doc
                    children: vec![Arc::new(n), Arc::new(node)],
                }
            }
        }
    }

    // TODO: add sibling before/after - requires node to be NON-ROOT (result?)
    // TODO: add child - Arc::downgrade(parent) to create ref
    // TODO: detach - weak.upgrade() to get parent, and then remove child
}

impl<B: Display, L: Display> Node<B, L> {
    const EMPTY: &'static str = "   ";
    const EDGE: &'static str = " └─";
    const PIPE: &'static str = " │ ";
    const BRANCH: &'static str = " ├─";

    pub fn write_tree(f: &mut dyn std::fmt::Write, node: &Self, title: Option<&str>) -> std::fmt::Result {
        let depths = match title {
            Some(title) => {
                writeln!(f, "{}", title)?;
                vec![1]
            }
            None => vec![],
        };
        Self::write_tree_inner(f, node, depths)
    }

    fn write_tree_inner(f: &mut dyn std::fmt::Write, node: &Self, depths: Vec<usize>) -> std::fmt::Result {
        // The connection to this node
        let mut this_prefix = String::new();
        // The connection to siblings of this node
        let mut sibling_prefix = String::new();

        // Iterate through the current depths to build up the right prefixes
        for (i, depth) in depths.iter().enumerate() {
            let last = i == depths.len() - 1;
            if *depth == 1 {
                this_prefix.push_str(if last { Self::EDGE } else { Self::EMPTY });
                sibling_prefix.push_str(Self::EMPTY);
            } else {
                this_prefix.push_str(if last { Self::BRANCH } else { Self::PIPE });
                sibling_prefix.push_str(Self::PIPE);
            }
        }

        write!(f, "{}", this_prefix)?;
        match node {
            Node::Branch { children, inner, .. } => {
                write!(f, "Branch")?;
                if let Some(inner) = inner {
                    writeln!(f, "({})", inner)?;
                } else {
                    writeln!(f)?;
                }
                let mut depth = children.len();
                for n in children {
                    let mut next_depths = depths.clone();
                    next_depths.push(depth);
                    depth -= 1;
                    Self::write_tree_inner(f, n, next_depths)?;
                }
            }
            Node::Leaves { children, inner, .. } => {
                write!(f, "Leaves")?;
                if let Some(inner) = inner {
                    writeln!(f, "({})", inner)?;
                } else {
                    writeln!(f)?;
                }
                for (i, leaf) in children.iter().enumerate() {
                    let last = i == children.len() - 1;
                    writeln!(
                        f,
                        "{}{}{}",
                        sibling_prefix,
                        if last { Self::EDGE } else { Self::BRANCH },
                        leaf
                    )?;
                }
            }
        }

        Ok(())
    }
}

impl<B: Display, L: Display> ToString for Node<B, L> {
    fn to_string(&self) -> String {
        let mut s = String::new();
        Node::write_tree(&mut s, self, None).expect("Failed to convert Node to String");
        s
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

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use super::*;

    #[test]
    fn append_child() {
        let mut root = Node::<usize, usize>::branch(Some(42));
        root.append_child(Node::branch(Some(1729)));
        assert_eq!(
            root.to_string(),
            indoc! {"
                Branch(42)
                 └─Branch(1729)
            "}
        );

        let mut root = Node::Leaves {
            parent: Weak::new(),
            inner: Some(42),
            children: vec![Leaf(1), Leaf(2), Leaf(3)],
        };
        root.append_child(Node::Leaves {
            parent: Weak::new(),
            inner: Some(1729),
            children: vec![Leaf(4), Leaf(5), Leaf(6)],
        });
        assert_eq!(
            root.to_string(),
            indoc! {"
                Branch
                 ├─Leaves(42)
                 │  ├─Leaf(1)
                 │  ├─Leaf(2)
                 │  └─Leaf(3)
                 └─Leaves(1729)
                    ├─Leaf(4)
                    ├─Leaf(5)
                    └─Leaf(6)
            "}
        );
    }

    #[test]
    #[ignore]
    fn asdf() {
        #[derive(Debug)]
        enum A {
            One,
            Two,
        }

        impl A {
            fn swap(&mut self) {
                match self {
                    A::One => {
                        *self = A::Two;
                    }
                    A::Two => {
                        *self = A::One;
                    }
                }
            }
        }

        use std::sync::Arc;

        let mut a = Arc::new(A::One);
        dbg!(&a);
        Arc::get_mut(&mut a).unwrap().swap();
        dbg!(&a);
    }

    #[test]
    fn leaves_no_children() {
        let root = Node::<char, char>::Branch {
            parent: Weak::new(),
            inner: None,
            children: vec![
                Arc::new(Node::Leaves {
                    parent: Weak::new(), /* TODO */
                    inner: None,
                    children: vec![],
                }),
                Arc::new(Node::Leaves {
                    parent: Weak::new(), /* TODO */
                    inner: None,
                    children: vec![],
                }),
            ],
        };

        let mut s = String::new();
        Node::write_tree(&mut s, &root, None).unwrap();
        assert_eq!(
            s,
            indoc! {"
                Branch
                 ├─Leaves
                 └─Leaves
            "}
        );
    }

    #[test]
    fn to_string() {
        let root = Node::Leaves {
            parent: Weak::new(), /* TODO */
            inner: Some('a'),
            children: vec![Leaf(0), Leaf(1), Leaf(2)],
        };

        let mut s = String::new();
        Node::write_tree(&mut s, &root, None).unwrap();
        assert_eq!(
            s,
            indoc! {"
                Leaves(a)
                 ├─Leaf(0)
                 ├─Leaf(1)
                 └─Leaf(2)
            "}
        );

        let mut s = String::new();
        Node::write_tree(&mut s, &root, Some("RootNode")).unwrap();
        assert_eq!(
            s,
            indoc! {"
                RootNode
                 └─Leaves(a)
                    ├─Leaf(0)
                    ├─Leaf(1)
                    └─Leaf(2)
            "}
        );
    }
}

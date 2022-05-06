#![allow(dead_code)]

use std::fmt::{Debug, Display};
use std::sync::{Arc, Weak};

// NOTE: I think this may be a lost cause... unless I go for unsafe code (managing pointers &
// mutability myself) which is something I wanted to avoid... Here's someone else's attempt:
// http://way-cooler.org/blog/2016/08/14/designing-a-bi-mutable-directional-tree-safely-in-rust.html

// TODO: operations - ie: add, detach, etc
//  (tree)detach: use global AtomicUsize as an internal id, search for parent who owns that child, perform detach there
//  (node)detach: need an unsafe pointer or a weakref to parent
//  (tree)add: TODO
//  (node)add: TODO
//  (tree)nest: TODO (probably combination of add + detach)
//  (node)nest: TODO (probably combination of add + detach)

pub struct Tree<B, L> {
    root: Arc<Node<B, L>>,
}

impl<B: Display, L: Display> Tree<B, L> {
    pub fn write_tree(f: &mut dyn std::fmt::Write, tree: &Tree<B, L>) -> std::fmt::Result {
        writeln!(f, "Root")?;
        Node::write_tree(f, &tree.root, Some("Root"))
    }
}

impl<B: Display, L: Display> ToString for Tree<B, L> {
    fn to_string(&self) -> String {
        let mut s = String::new();
        Node::write_tree(&mut s, &self.root, Some("Root")).expect("Failed to convert Tree to String!");
        s
    }
}

pub enum NodeParent<B, L> {
    Root(Weak<Node<B, L>>),
    Node(Weak<Node<B, L>>),
}

impl<B, L> NodeParent<B, L> {
    pub fn as_root(&self) -> Option<&Weak<Node<B, L>>> {
        match self {
            NodeParent::Root(n) => Some(n),
            NodeParent::Node(_) => None,
        }
    }

    pub fn as_node(&self) -> Option<&Weak<Node<B, L>>> {
        match self {
            NodeParent::Node(n) => Some(n),
            NodeParent::Root(_) => None,
        }
    }
}

pub enum NodeChildren<B, L> {
    Branch(Vec<Arc<Node<B, L>>>),
    Leaves(Vec<Leaf<L>>),
}

impl<B, L> NodeChildren<B, L> {
    pub fn as_branch(&self) -> Option<&[Arc<Node<B, L>>]> {
        match self {
            NodeChildren::Branch(children) => Some(children),
            NodeChildren::Leaves(_) => None,
        }
    }

    pub fn as_leaves(&self) -> Option<&[Leaf<L>]> {
        match self {
            NodeChildren::Leaves(children) => Some(children),
            NodeChildren::Branch(_) => None,
        }
    }
}

pub struct Node<B, L> {
    inner: Option<B>,
    parent: NodeParent<B, L>,
    children: NodeChildren<B, L>,
}

impl<B, L> Node<B, L> {
    pub fn is_root(&self) -> bool {
        matches!(self.parent, NodeParent::Root(_))
    }

    // TODO: need append leaf?
    pub fn append_branch(&mut self, inner: Option<B>) {
        // TODO: doc
        let weak_ref_to_me = match &self.parent {
            // TODO: does this work or am I mad?
            NodeParent::Root(n) => n.clone(),
            // SAFETY: TODO
            NodeParent::Node(n) => match &n.upgrade().unwrap().as_ref().children {
                // SAFETY: TODO
                NodeChildren::Leaves(_) => unreachable!(),
                NodeChildren::Branch(children) => {
                    // SAFETY: TODO
                    let me = children
                        .iter()
                        // ???: as *const _ ??? TODO: does this actually work how I think it does?
                        .find(|c| c.as_ref() as *const _ == self as *const _)
                        .unwrap();
                    Arc::downgrade(me)
                }
            },
        };

        let new_node = Node {
            inner,
            parent: NodeParent::Node(weak_ref_to_me.clone()),
            children: NodeChildren::Branch(vec![]),
        };

        // If this is already a branch node, then we just add the new node
        if let NodeChildren::Branch(children) = &mut self.children {
            children.push(Arc::new(new_node));
            return;
        }

        // If it's a leaves node, then we need to transform it into a branch node, and put its
        // contents in a new child node, and then add the new node there too
        let my_new_place = Node {
            parent: NodeParent::Node(weak_ref_to_me),
            // TODO: doc
            inner: self.inner.take(),
            children: NodeChildren::Leaves(match &mut self.children {
                // SAFETY: TODO
                NodeChildren::Branch(_) => unreachable!(),
                NodeChildren::Leaves(children) => children.drain(..).collect(),
            }),
        };

        self.children = NodeChildren::Branch(vec![Arc::new(my_new_place), Arc::new(new_node)]);
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

        write!(
            f,
            "{}{}",
            this_prefix,
            match node.children {
                NodeChildren::Branch(_) => "Branch",
                NodeChildren::Leaves(_) => "Leaves",
            }
        )?;

        if let Some(inner) = &node.inner {
            writeln!(f, "({})", inner)?;
        } else {
            writeln!(f)?;
        }

        match &node.children {
            NodeChildren::Branch(children) => {
                let mut depth = children.len();
                for n in children {
                    let mut next_depths = depths.clone();
                    next_depths.push(depth);
                    depth -= 1;
                    Self::write_tree_inner(f, n, next_depths)?;
                }
            }
            NodeChildren::Leaves(children) => {
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
    use std::sync::{Arc, Weak};

    use indoc::indoc;

    use super::*;

    #[test]
    fn append_branch() {
        let mut root: Node<i32, i32> = Node {
            inner: Some(42),
            parent: NodeParent::Root(Weak::new()),
            children: NodeChildren::Branch(vec![]),
        };
        root.append_branch(Some(1729));
        assert_eq!(
            root.to_string(),
            indoc! {"
                Branch(42)
                 └─Branch(1729)
            "}
        );
        let child = &root.children.as_branch().unwrap()[0];
        assert_eq!(Some(1729), child.inner);
        // FIXME: parent ref isn't working
        assert_eq!(Some(42), child.parent.as_node().unwrap().upgrade().unwrap().inner);

        let mut root = Node {
            inner: Some(42),
            parent: NodeParent::Root(Weak::new()),
            children: NodeChildren::Leaves(vec![Leaf(1), Leaf(2), Leaf(3)]),
        };
        root.append_branch(Some(1729));
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
    fn leaves_no_children() {
        let root: Node<i32, i32> = Node {
            inner: None,
            parent: NodeParent::Root(Weak::new()),
            children: NodeChildren::Branch(vec![
                Arc::new(Node {
                    parent: NodeParent::Root(Weak::new()), /* TODO */
                    inner: None,
                    children: NodeChildren::Leaves(vec![]),
                }),
                Arc::new(Node {
                    parent: NodeParent::Root(Weak::new()), /* TODO */
                    inner: None,
                    children: NodeChildren::Leaves(vec![]),
                }),
            ]),
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
        let root = Node {
            parent: NodeParent::Root(Weak::new()), /* TODO */
            inner: Some('a'),
            children: NodeChildren::Leaves(vec![Leaf(0), Leaf(1), Leaf(2)]),
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

    #[test]
    fn it_works() {
        // TODO: easier ways to build these
        let tree = Tree {
            root: Arc::new(Node {
                inner: Some('a'),
                parent: NodeParent::Root(Weak::new()),
                children: NodeChildren::Leaves(vec![Leaf(0), Leaf(1), Leaf(2)]),
            }),
        };
        assert_eq!(
            tree.to_string(),
            indoc! {"
                Root
                 └─Leaves(a)
                    ├─Leaf(0)
                    ├─Leaf(1)
                    └─Leaf(2)
            "}
        );

        let tree = Tree {
            root: Arc::new(Node {
                inner: Some('a'),
                parent: NodeParent::Root(Weak::new()), /* TODO */
                children: NodeChildren::Branch(vec![
                    Arc::new(Node {
                        inner: Some('b'),
                        parent: NodeParent::Root(Weak::new()), /* TODO */
                        children: NodeChildren::Leaves(vec![Leaf(0), Leaf(1)]),
                    }),
                    Arc::new(Node {
                        inner: Some('c'),
                        parent: NodeParent::Root(Weak::new()), /* TODO */
                        children: NodeChildren::Leaves(vec![Leaf(2)]),
                    }),
                    Arc::new(Node {
                        inner: Some('d'),
                        parent: NodeParent::Root(Weak::new()), /* TODO */
                        children: NodeChildren::Leaves(vec![Leaf(3), Leaf(4), Leaf(5)]),
                    }),
                ]),
            }),
        };
        assert_eq!(
            tree.to_string(),
            indoc! {"
                Root
                 └─Branch(a)
                    ├─Leaves(b)
                    │  ├─Leaf(0)
                    │  └─Leaf(1)
                    ├─Leaves(c)
                    │  └─Leaf(2)
                    └─Leaves(d)
                       ├─Leaf(3)
                       ├─Leaf(4)
                       └─Leaf(5)
            "}
        );

        let tree = Tree {
            root: Arc::new(Node {
                inner: Some('a'),
                parent: NodeParent::Root(Weak::new()),
                children: NodeChildren::Branch(vec![
                    Arc::new(Node {
                        parent: NodeParent::Root(Weak::new()), /* TODO */
                        inner: Some('b'),
                        children: NodeChildren::Branch(vec![Arc::new(Node {
                            parent: NodeParent::Root(Weak::new()), /* TODO */
                            inner: Some('c'),
                            children: NodeChildren::Leaves(vec![Leaf(0), Leaf(1), Leaf(2)]),
                        })]),
                    }),
                    Arc::new(Node {
                        parent: NodeParent::Root(Weak::new()), /* TODO */
                        inner: Some('d'),
                        children: NodeChildren::Leaves(vec![Leaf(3), Leaf(4)]),
                    }),
                    Arc::new(Node {
                        parent: NodeParent::Root(Weak::new()), /* TODO */
                        inner: Some('e'),
                        children: NodeChildren::Branch(vec![Arc::new(Node {
                            parent: NodeParent::Root(Weak::new()), /* TODO */
                            inner: Some('f'),
                            children: NodeChildren::Leaves(vec![Leaf(5), Leaf(6)]),
                        })]),
                    }),
                ]),
            }),
        };
        assert_eq!(
            tree.to_string(),
            indoc! {"
                Root
                 └─Branch(a)
                    ├─Branch(b)
                    │  └─Leaves(c)
                    │     ├─Leaf(0)
                    │     ├─Leaf(1)
                    │     └─Leaf(2)
                    ├─Leaves(d)
                    │  ├─Leaf(3)
                    │  └─Leaf(4)
                    └─Branch(e)
                       └─Leaves(f)
                          ├─Leaf(5)
                          └─Leaf(6)
            "}
        );
    }
}

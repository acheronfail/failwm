#![allow(dead_code)]

mod node;

use std::fmt::Display;

use self::node::Node;

// TODO: operations - ie: add, detach, etc
//  (tree)detach: use global AtomicUsize as an internal id, search for parent who owns that child, perform detach there
//  (node)detach: need an unsafe pointer or a weakref to parent
//  (tree)add: TODO
//  (node)add: TODO
//  (tree)nest: TODO (probably combination of add + detach)
//  (node)nest: TODO (probably combination of add + detach)

pub struct Tree<B, L> {
    root: Node<B, L>,
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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Weak};

    use indoc::indoc;

    use self::node::*;
    use super::*;

    #[test]
    fn it_works() {
        // TODO: easier ways to build these
        let tree = Tree {
            root: Node::Leaves {
                parent: Weak::new(), /* TODO */
                inner: Some('a'),
                children: vec![Leaf(0), Leaf(1), Leaf(2)],
            },
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
            root: Node::Branch {
                parent: Weak::new(), /* TODO */
                inner: Some('a'),
                children: vec![
                    Arc::new(Node::Leaves {
                        parent: Weak::new(), /* TODO */
                        inner: Some('b'),
                        children: vec![Leaf(0), Leaf(1)],
                    }),
                    Arc::new(Node::Leaves {
                        parent: Weak::new(), /* TODO */
                        inner: Some('c'),
                        children: vec![Leaf(2)],
                    }),
                    Arc::new(Node::Leaves {
                        parent: Weak::new(), /* TODO */
                        inner: Some('d'),
                        children: vec![Leaf(3), Leaf(4), Leaf(5)],
                    }),
                ],
            },
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
            root: Node::Branch {
                parent: Weak::new(), /* TODO */
                inner: Some('a'),
                children: vec![
                    Arc::new(Node::Branch {
                        parent: Weak::new(), /* TODO */
                        inner: Some('b'),
                        children: vec![Arc::new(Node::Leaves {
                            parent: Weak::new(), /* TODO */
                            inner: Some('c'),
                            children: vec![Leaf(0), Leaf(1), Leaf(2)],
                        })],
                    }),
                    Arc::new(Node::Leaves {
                        parent: Weak::new(), /* TODO */
                        inner: Some('d'),
                        children: vec![Leaf(3), Leaf(4)],
                    }),
                    Arc::new(Node::Branch {
                        parent: Weak::new(), /* TODO */
                        inner: Some('e'),
                        children: vec![Arc::new(Node::Leaves {
                            parent: Weak::new(), /* TODO */
                            inner: Some('f'),
                            children: vec![Leaf(5), Leaf(6)],
                        })],
                    }),
                ],
            },
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

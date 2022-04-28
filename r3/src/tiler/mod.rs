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

impl<B: Default, L> Tree<B, L> {
    pub fn new() -> Self {
        Self {
            root: Node::leaves(B::default()),
        }
    }
}

impl<B: Display, L: Display> Tree<B, L> {
    pub fn write_tree(f: &mut dyn std::fmt::Write, tree: &Tree<B, L>) -> std::fmt::Result {
        writeln!(f, "Root")?;
        Self::write_tree_inner(f, &tree.root, vec![1])
    }

    fn write_tree_inner(f: &mut dyn std::fmt::Write, node: &Node<B, L>, depths: Vec<usize>) -> std::fmt::Result {
        const EMPTY: &str = "   ";
        const EDGE: &str = " └─";
        const PIPE: &str = " │ ";
        const BRANCH: &str = " ├─";

        // The connection to this node
        let mut this_prefix = String::new();
        // The connection to siblings of this node
        let mut sibling_prefix = String::new();

        // Iterate through the current depths to build up the right prefixes
        for (i, depth) in depths.iter().enumerate() {
            let last = i == depths.len() - 1;
            if *depth == 1 {
                this_prefix.push_str(if last { EDGE } else { EMPTY });
                sibling_prefix.push_str(EMPTY);
            } else {
                this_prefix.push_str(if last { BRANCH } else { PIPE });
                sibling_prefix.push_str(PIPE);
            }
        }

        write!(f, "{}", this_prefix)?;
        match node {
            Node::Branch { children, inner } => {
                let mut depth = children.len();
                writeln!(f, "Branch({})", inner)?;
                for n in children {
                    let mut next_depths = depths.clone();
                    next_depths.push(depth);
                    depth -= 1;
                    Self::write_tree_inner(f, n, next_depths)?;
                }
            }
            Node::Leaves { children, inner } => {
                writeln!(f, "Leaves({})", inner)?;
                for (i, leaf) in children.iter().enumerate() {
                    let last = i == children.len() - 1;
                    writeln!(f, "{}{}{}", sibling_prefix, if last { EDGE } else { BRANCH }, leaf)?;
                }
            }
        }

        Ok(())
    }
}

impl<B: Display, L: Display> ToString for Tree<B, L> {
    fn to_string(&self) -> String {
        let mut s = String::new();
        Tree::write_tree(&mut s, self).expect("Failed to convert Tree to String!");
        s
    }
}

#[cfg(test)]
mod tests {
    use indoc::indoc;

    use self::node::*;
    use super::*;

    #[test]
    fn it_works() {
        // TODO: easier ways to build these
        let tree = Tree {
            root: Node::Leaves {
                inner: 'a',
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
                inner: 'a',
                children: vec![
                    Node::Leaves {
                        inner: 'b',
                        children: vec![Leaf(0), Leaf(1)],
                    },
                    Node::Leaves {
                        inner: 'c',
                        children: vec![Leaf(2)],
                    },
                    Node::Leaves {
                        inner: 'd',
                        children: vec![Leaf(3), Leaf(4), Leaf(5)],
                    },
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
                inner: 'a',
                children: vec![
                    Node::Branch {
                        inner: 'b',
                        children: vec![Node::Leaves {
                            inner: 'c',
                            children: vec![Leaf(0), Leaf(1), Leaf(2)],
                        }],
                    },
                    Node::Leaves {
                        inner: 'd',
                        children: vec![Leaf(3), Leaf(4)],
                    },
                    Node::Branch {
                        inner: 'e',
                        children: vec![Node::Leaves {
                            inner: 'f',
                            children: vec![Leaf(5), Leaf(6)],
                        }],
                    },
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

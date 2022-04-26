#![allow(dead_code)]

use std::fmt::{Debug, Display};

// TODO: operations - ie: add, detach, etc
//  (tree)detach: use global AtomicUsize as an internal id, search for parent who owns that child, perform detach there
//  (node)detach: need an unsafe pointer or a weakref to parent
//  (tree)add: TODO
//  (node)add: TODO
//  (tree)nest: TODO (probably combination of add + detach)
//  (node)nest: TODO (probably combination of add + detach)

pub struct Tree<T> {
    root: Box<Node<T>>,
}

impl<T: Display> Tree<T> {
    pub fn new() -> Self {
        Self {
            root: Box::new(Node::leaves()),
        }
    }

    pub fn write_tree(f: &mut dyn std::fmt::Write, tree: &Tree<T>) -> std::fmt::Result {
        writeln!(f, "Root")?;
        Self::write_tree_inner(f, tree.root.as_ref(), vec![1])
    }

    fn write_tree_inner(f: &mut dyn std::fmt::Write, node: &Node<T>, depths: Vec<usize>) -> std::fmt::Result {
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
            Node::Branch { children } => {
                let mut depth = children.len();
                writeln!(f, "{}", "Branch")?;
                for n in children {
                    let mut next_depths = depths.clone();
                    next_depths.push(depth);
                    depth -= 1;
                    Self::write_tree_inner(f, n, next_depths)?;
                }
            }
            Node::Leaves { children } => {
                writeln!(f, "{}", "Leaves")?;
                for (i, leaf) in children.iter().enumerate() {
                    let last = i == children.len() - 1;
                    writeln!(f, "{}{}{}", sibling_prefix, if last { EDGE } else { BRANCH }, leaf)?;
                }
            }
        }

        Ok(())
    }
}

impl<T: Display> ToString for Tree<T> {
    fn to_string(&self) -> String {
        let mut s = String::new();
        Tree::write_tree(&mut s, self).expect("Failed to convert Tree to String!");
        s
    }
}

#[derive(Debug)]
pub enum Node<T> {
    Branch { children: Vec<Box<Node<T>>> },
    Leaves { children: Vec<Leaf<T>> },
}

impl<T> Node<T> {
    pub fn leaves() -> Self {
        Self::Leaves { children: vec![] }
    }

    pub fn branch() -> Self {
        Self::Branch { children: vec![] }
    }
}

#[derive(Debug)]
pub struct Leaf<T>(T);

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
    fn it_works() {
        // TODO: easier ways to build these
        let tree = Tree {
            root: Box::new(Node::Leaves {
                children: vec![Leaf(0), Leaf(1), Leaf(2)],
            }),
        };
        assert_eq!(
            tree.to_string(),
            indoc! {"
                Root
                 └─Leaves
                    ├─Leaf(0)
                    ├─Leaf(1)
                    └─Leaf(2)
            "}
        );

        let tree = Tree {
            root: Box::new(Node::Branch {
                children: vec![
                    Box::new(Node::Leaves {
                        children: vec![Leaf(0), Leaf(1)],
                    }),
                    Box::new(Node::Leaves {
                        children: vec![Leaf(2)],
                    }),
                    Box::new(Node::Leaves {
                        children: vec![Leaf(3), Leaf(4), Leaf(5)],
                    }),
                ],
            }),
        };
        assert_eq!(
            tree.to_string(),
            indoc! {"
                Root
                 └─Branch
                    ├─Leaves
                    │  ├─Leaf(0)
                    │  └─Leaf(1)
                    ├─Leaves
                    │  └─Leaf(2)
                    └─Leaves
                       ├─Leaf(3)
                       ├─Leaf(4)
                       └─Leaf(5)
            "}
        );

        let tree = Tree {
            root: Box::new(Node::Branch {
                children: vec![
                    Box::new(Node::Branch {
                        children: vec![Box::new(Node::Leaves {
                            children: vec![Leaf(0), Leaf(1), Leaf(2)],
                        })],
                    }),
                    Box::new(Node::Leaves {
                        children: vec![Leaf(3), Leaf(4)],
                    }),
                    Box::new(Node::Branch {
                        children: vec![Box::new(Node::Leaves {
                            children: vec![Leaf(5), Leaf(6)],
                        })],
                    }),
                ],
            }),
        };
        assert_eq!(
            tree.to_string(),
            indoc! {"
                Root
                 └─Branch
                    ├─Branch
                    │  └─Leaves
                    │     ├─Leaf(0)
                    │     ├─Leaf(1)
                    │     └─Leaf(2)
                    ├─Leaves
                    │  ├─Leaf(3)
                    │  └─Leaf(4)
                    └─Branch
                       └─Leaves
                          ├─Leaf(5)
                          └─Leaf(6)
            "}
        );
    }
}

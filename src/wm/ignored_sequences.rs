use std::{collections::BinaryHeap, time::Instant};

#[derive(Debug, PartialEq, Eq)]
struct Inner {
    sequence: u16,
    response_type: Option<u32>,
    store_time: Instant,
}

impl Inner {
    pub fn new(sequence: u16) -> Inner {
        Inner {
            sequence,
            response_type: None,
            store_time: Instant::now(),
        }
    }

    pub fn new_with_type(sequence: u16, response_type: u32) -> Inner {
        Inner {
            sequence,
            response_type: Some(response_type),
            store_time: Instant::now(),
        }
    }
}

// Inner is sorted by store time
impl PartialOrd for Inner {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.store_time.partial_cmp(&other.store_time)
    }
}

// Inner is sorted by store time
impl Ord for Inner {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.store_time.cmp(&other.store_time)
    }
}

pub struct IgnoredSequences {
    // Tuple of (sequence, Option<ResponseType>, timestamp)
    heap: BinaryHeap<Inner>,
}

impl IgnoredSequences {
    pub fn new() -> IgnoredSequences {
        IgnoredSequences {
            heap: BinaryHeap::new(),
        }
    }

    pub fn add(&mut self, sequence: u16) {
        self.heap.push(Inner::new(sequence));
    }

    pub fn add_with_type(&mut self, sequence: u16, response_type: u32) {
        self.heap.push(Inner::new_with_type(sequence, response_type));
    }

    pub fn is_ignored(&mut self, sequence: u16, response_type: u32) -> bool {
        // Clean out any old items automatically
        let now = Instant::now();
        while let Some(inner) = self.heap.peek() {
            match now.duration_since(inner.store_time).as_secs() >= 5 {
                true => {
                    self.heap.pop();
                }
                false => break,
            }
        }

        // Check if the given event is ignored
        for inner in &self.heap {
            if inner.sequence != sequence {
                continue;
            }
            if let Some(inner_response_type) = inner.response_type {
                if inner_response_type != response_type {
                    continue;
                }
            }

            return true;
        }

        false
    }
}

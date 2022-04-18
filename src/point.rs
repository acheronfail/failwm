use std::ops::{Sub, Add};

#[derive(Debug, Clone, Copy)]
pub struct Point {
    pub x: i16,
    pub y: i16
}

impl Point {
    pub fn new(x: i16, y: i16) -> Point {
        Point { x, y }
    }
}

impl From<(i16, i16)> for Point {
    fn from((x, y): (i16, i16)) -> Self {
        Point::new(x, y)
    }
}

impl Add for Point {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Point::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Point {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Point::new(self.x - rhs.x, self.y - rhs.y)
    }
}
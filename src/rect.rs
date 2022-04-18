use crate::point::Point;

#[derive(Debug, Clone, Copy)]
pub enum Quadrant {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy)]
pub struct Rect {
    pub x: i16,
    pub y: i16,
    pub width: u16,
    pub height: u16,
}

impl Rect {
    pub fn new(x: i16, y: i16, width: u16, height: u16) -> Rect {
        Rect { x, y, width, height }
    }

    pub fn contains(&self, point: &Point) -> bool {
        (self.x <= point.x && point.x <= self.x + self.width as i16)
            && (self.y <= point.y && point.y <= self.y + self.height as i16)
    }

    pub fn quadrant(&self, point: &Point) -> Option<Quadrant> {
        let w = (self.width / 2) as i16;
        let h = (self.height / 2) as i16;
        if !self.contains(point) {
            None
        } else if point.x > w && point.y > h {
            Some(Quadrant::BottomRight)
        } else if point.x > w && point.y <= h {
            Some(Quadrant::TopRight)
        } else if point.x <= w && point.y > h {
            Some(Quadrant::BottomLeft)
        } else {
            Some(Quadrant::TopLeft)
        }
    }
}

impl From<(i16, i16, u16, u16)> for Rect {
    fn from((x, y, w, h): (i16, i16, u16, u16)) -> Self {
        Rect::new(x, y, w, h)
    }
}

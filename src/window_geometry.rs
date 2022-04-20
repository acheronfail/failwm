use crate::point::Point;

#[derive(Debug, Clone, Copy)]
pub enum Quadrant {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy)]
pub struct WindowGeometry {
    /// X coord relative to parent
    pub x: i16,
    /// Y coord relative to parent
    pub y: i16,
    /// Width
    pub w: u16,
    /// Height
    pub h: u16,
    /// Border Width
    pub bw: u16,
}

impl WindowGeometry {
    pub fn new(x: i16, y: i16, width: u16, height: u16, border_width: u16) -> WindowGeometry {
        WindowGeometry {
            x,
            y,
            w: width,
            h: height,
            bw: border_width,
        }
    }

    /// Full width, including the border
    pub fn full_width(&self) -> u16 {
        self.w + (self.bw * 2)
    }

    /// Full height, including the border
    pub fn full_height(&self) -> u16 {
        self.h + (self.bw * 2)
    }

    /// Does this window (including border) contain the point?
    pub fn contains(&self, point: &Point) -> bool {
        let end_x = self.x + self.full_width() as i16;
        let end_y = self.y + self.full_height() as i16;
        (self.x <= point.x && point.x <= end_x) && (self.y <= point.y && point.y <= end_y)
    }

    pub fn quadrant(&self, point: &Point) -> Option<Quadrant> {
        let horizonal_bound = self.x + (self.full_width() / 2) as i16;
        let vertical_bound = self.y + (self.full_height() / 2) as i16;
        if !self.contains(point) {
            None
        } else if point.x > horizonal_bound && point.y > vertical_bound {
            Some(Quadrant::BottomRight)
        } else if point.x > horizonal_bound && point.y <= vertical_bound {
            Some(Quadrant::TopRight)
        } else if point.x <= horizonal_bound && point.y > vertical_bound {
            Some(Quadrant::BottomLeft)
        } else {
            Some(Quadrant::TopLeft)
        }
    }
}

impl From<(i16, i16, u16, u16)> for WindowGeometry {
    fn from((x, y, w, h): (i16, i16, u16, u16)) -> Self {
        WindowGeometry::new(x, y, w, h, 0)
    }
}

impl From<(i16, i16, u16, u16, u16)> for WindowGeometry {
    fn from((x, y, w, h, bw): (i16, i16, u16, u16, u16)) -> Self {
        WindowGeometry::new(x, y, w, h, bw)
    }
}

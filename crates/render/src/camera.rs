use glam::{Mat4, Vec3, Vec4};

/// Orbiting orthographic camera for the low-resolution, pixelated 3D look. World units are meters; sim (x, y) maps to
/// render (x, 0, y). pitch 90 deg = top-down, ~45 deg + yaw = free orbit. One texel of the picture is 1/16 m and is drawn `scale()` screen pixels wide, so zooming
/// changes the integer pixel size, never the art.
#[derive(Clone, Copy, Debug)]
pub struct Camera {
    pub target: Vec3,
    pub pitch_deg: f32,
    pub yaw_deg: f32,
    pub zoom: f32,
    /// Window size in pixels (the game keeps it current).
    pub win: (u32, u32),
}

fn even_ceil(v: u32) -> u32 {
    v.div_ceil(2) * 2
}

impl Camera {
    pub const PITCH_TOP: f32 = 90.0;
    pub const PITCH_ISO: f32 = 45.0;
    /// Lowest tilt the player can orbit down to.
    pub const PITCH_MIN: f32 = 28.0;
    /// How much finer the picture is than the painted art's 16 texels per metre: the world keeps its size on screen,
    /// only the texels get smaller.
    pub const RES: f32 = 4.0 / 3.0;
    /// Texels per meter.
    pub const PPM: f32 = 16.0 * Self::RES;
    const DIST: f32 = 150.0;

    pub fn new() -> Self {
        Self { target: Vec3::ZERO, pitch_deg: Self::PITCH_ISO, yaw_deg: 45.0, zoom: 1.0, win: (1280, 720) }
    }

    /// Screen pixels per texel (zoom 1 = 3 / RES = 2.25). Zoom still moves in whole steps of the old scale.
    pub fn scale(&self) -> f32 {
        ((self.zoom * 3.0).round()).clamp(2.0, 6.0) / Self::RES
    }

    /// Size of the picture in texels (even, so the centre is a texel corner).
    pub fn tex_dims(&self) -> (u32, u32) {
        let s = self.scale();
        (even_ceil((self.win.0 as f32 / s).ceil() as u32), even_ceil((self.win.1 as f32 / s).ceil() as u32))
    }

    /// Fraction of the drawn picture's width and height that the window shows.
    fn shown(&self) -> (f32, f32) {
        let (sw, sh) = self.tex_dims();
        let s = self.scale();
        (self.win.0 as f32 / (sw as f32 * s), self.win.1 as f32 / (sh as f32 * s))
    }

    fn basis(&self) -> (Vec3, Vec3) {
        let (p, y) = (self.pitch_deg.to_radians(), self.yaw_deg.to_radians());
        let f = -Vec3::new(p.cos() * y.sin(), p.sin(), p.cos() * y.cos());
        let r = Vec3::new(y.cos(), 0.0, -y.sin());
        (f, r.cross(f))
    }

    /// Copy whose target sits on the texel grid, plus how far the real target is off that grid in texels
    /// (right, up). Everything drawn from the snapped camera is locked to the pixel grid while the camera slides;
    /// the remainder is applied when the small picture is enlarged.
    pub fn snapped(&self) -> (Camera, [f32; 2]) {
        let u = 1.0 / Self::PPM;
        let (f, up) = self.basis();
        let r = f.cross(up);
        let (tr, tu) = (self.target.dot(r), self.target.dot(up));
        let (dr, du) = (tr - (tr / u).round() * u, tu - (tu / u).round() * u);
        let mut cam = *self;
        cam.target -= r * dr + up * du;
        (cam, [dr / u, du / u])
    }

    /// Where a world point lands on screen, in the window's normalized device coordinates (-1..1, y up).
    pub fn project(&self, _aspect: f32, p: Vec3) -> [f32; 2] {
        let v = self.view_proj(1.0) * p.extend(1.0);
        let (fx, fy) = self.shown();
        [v.x / v.w / fx, v.y / v.w / fy]
    }

    pub fn view_proj(&self, _aspect: f32) -> Mat4 {
        let (f, u) = self.basis();
        let eye = self.target - f * Self::DIST;
        let view = glam::camera::rh::view::look_to_mat4(eye, f, u);
        let (sw, sh) = self.tex_dims();
        let (hw, hh) = (sw as f32 / (2.0 * Self::PPM), sh as f32 / (2.0 * Self::PPM));
        let proj = glam::camera::rh::proj::directx::orthographic(-hw, hw, -hh, hh, 0.1, Self::DIST * 3.0);
        proj * view
    }

    /// Ground (y = 0) position under a point in the window's normalized device coords (-1..1, y up).
    pub fn ground_at(&self, ndc_x: f32, ndc_y: f32, _aspect: f32) -> Option<Vec3> {
        let (fx, fy) = self.shown();
        let inv = self.view_proj(1.0).inverse();
        let (nx, ny) = (ndc_x * fx, ndc_y * fy);
        let near = inv * Vec4::new(nx, ny, 0.0, 1.0);
        let far = inv * Vec4::new(nx, ny, 1.0, 1.0);
        let (a, b) = (near.truncate() / near.w, far.truncate() / far.w);
        let d = b - a;
        if d.y.abs() < 1e-6 {
            return None;
        }
        Some(a + d * (-a.y / d.y))
    }

    /// Like `ground_at`, but for ground that rises and falls: `height(x, z)` is the surface height in metres.
    pub fn ground_at_height(&self, ndc_x: f32, ndc_y: f32, height: impl Fn(f32, f32) -> f32) -> Option<Vec3> {
        let (fx, fy) = self.shown();
        let inv = self.view_proj(1.0).inverse();
        let (nx, ny) = (ndc_x * fx, ndc_y * fy);
        let (near, far) = (inv * Vec4::new(nx, ny, 0.0, 1.0), inv * Vec4::new(nx, ny, 1.0, 1.0));
        let (a, b) = (near.truncate() / near.w, far.truncate() / far.w);
        let d = b - a;
        if d.y.abs() < 1e-6 {
            return None;
        }
        let mut y = 0.0;
        let mut p = a;
        for _ in 0..5 {
            p = a + d * ((y - a.y) / d.y);
            y = height(p.x, p.z);
        }
        Some(p)
    }

    /// Pan on the ground plane (right, forward in meters).
    pub fn pan(&mut self, right: f32, forward: f32) {
        let y = self.yaw_deg.to_radians();
        self.target.x += right * y.cos() - forward * y.sin();
        self.target.z += -right * y.sin() - forward * y.cos();
    }
}

impl Default for Camera {
    fn default() -> Self {
        Self::new()
    }
}

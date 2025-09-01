#[allow(dead_code)]
#[derive(Clone, Copy, Debug)]
pub struct Meter {
    pub rms: f32,
    pub peak: f32,
}

#[derive(Clone, Debug)]
pub struct Spectrum {
    pub bands: Vec<f32>,
    pub bands_linear: Vec<f32>,
}

//! Where a map sits on the earth, as its `<header>` says.

/// A map's `<header><geoReference>` and `<header><offset>`, as the file
/// gives them. The crate applies neither: [`Lane`](crate::Lane) points stay
/// in the file's own frame. See [Geo reference](crate#geo-reference).
#[derive(Debug, Clone, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GeoReference {
    /// The `<geoReference>` PROJ string, or `None` for a local frame.
    pub proj: Option<String>,
    /// The `<offset>`, if the file has one.
    pub offset: Option<GeoOffset>,
}

/// A `<header><offset>`. The spec takes a point from the map's frame to the
/// PROJ frame by rotating it by `hdg` about z, then adding `x`, `y`, `z`.
/// Some exporters write the opposite sign, so check a map before relying on
/// it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct GeoOffset {
    /// `x`, in metres.
    pub x: f64,
    /// `y`, in metres.
    pub y: f64,
    /// `z`, in metres.
    pub z: f64,
    /// `hdg`, in radians.
    pub hdg: f64,
}

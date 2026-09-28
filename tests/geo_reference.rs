//! The geo reference: `<header><geoReference>` and `<header><offset>`, kept
//! as the file gives them.

use libopendrive::{load_file, load_str, GeoOffset, GeoReference, RoadNetwork};

/// A one-lane road under a `<header>` holding `header`.
fn map(header: &str) -> RoadNetwork {
    load_str(&format!(
        r#"<?xml version="1.0"?>
<OpenDRIVE>
  <header revMajor="1" revMinor="9">{header}</header>
  <road length="40" id="1" junction="-1">
    <planView><geometry s="0" x="0" y="0" hdg="0" length="40"><line/></geometry></planView>
    <lanes><laneSection s="0">
      <right><lane id="-1" type="driving"><width sOffset="0" a="4"/></lane></right>
    </laneSection></lanes>
  </road>
</OpenDRIVE>"#
    ))
    .expect("the map loads")
}

const UTM: &str = "+proj=utm +zone=32 +ellps=GRS80 +towgs84=0,0,0,0,0,0,0 +units=m +no_defs";

#[test]
fn the_proj_string_is_read_from_cdata_and_trimmed() {
    let net = map(&format!(
        "<geoReference>\n      <![CDATA[{UTM}]]>\n    </geoReference>"
    ));
    assert_eq!(net.geo_reference().proj.as_deref(), Some(UTM));
    assert_eq!(net.geo_reference().offset, None);
}

#[test]
fn the_offset_is_kept_unapplied() {
    let net = map(r#"<offset x="-387907.55" y="-3948051.8" z="1.5" hdg="0.25"/>"#);
    assert_eq!(
        net.geo_reference().offset,
        Some(GeoOffset {
            x: -387907.55,
            y: -3948051.8,
            z: 1.5,
            hdg: 0.25,
        })
    );
    let start = net.lanes()[0].center.point_at(0.0);
    assert!(start.x.abs() < 1e-3 && start.z.abs() < 1e-3, "{start:?}");
}

#[test]
fn a_missing_or_unreadable_offset_attribute_reads_as_zero() {
    let net = map(r#"<offset x="12" y="north" z="3"/>"#);
    assert_eq!(
        net.geo_reference().offset,
        Some(GeoOffset {
            x: 12.0,
            y: 0.0,
            z: 3.0,
            hdg: 0.0,
        })
    );
}

#[test]
fn a_blank_or_missing_geo_reference_is_none() {
    assert_eq!(*map("").geo_reference(), GeoReference::default());
    assert_eq!(
        map("<geoReference>  </geoReference>").geo_reference().proj,
        None
    );
}

#[test]
fn town07_keeps_its_roadrunner_proj_string() {
    let net = load_file("tests/data/town07.xodr").expect("town07 loads");
    assert_eq!(
        net.geo_reference().proj.as_deref(),
        Some("+lat_0=4.9000000000000000e+1 +lon_0=8.0000000000000000e+0")
    );
}

#[cfg(feature = "serde")]
#[test]
fn the_geo_reference_survives_serde() {
    let net = map(&format!(
        r#"<geoReference><![CDATA[{UTM}]]></geoReference><offset x="1" y="2" z="3" hdg="0"/>"#
    ));
    let json = serde_json::to_string(&net).expect("serialize");
    let back: RoadNetwork = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, net);
    assert_eq!(back.geo_reference().proj.as_deref(), Some(UTM));
}

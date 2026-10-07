//! The demo's blockout test course, in Unreal units (cm, Z up, X forward).

use dis_motion::boxworld::BoxWorld;
use dis_motion::Vec3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Ground,
    Wall,
    Ledge,
    Stairs,
    Roof,
    Guard,
    Ladder,
    Water,
}

pub struct Piece {
    pub min: Vec3,
    pub max: Vec3,
    pub kind: Kind,
}

pub struct Level {
    pub world: BoxWorld,
    pub pieces: Vec<Piece>,
    pub spawn: Vec3,
    pub spawn_yaw: f32,
}

impl Level {
    fn solid(&mut self, min: [f32; 3], max: [f32; 3], kind: Kind) {
        let (min, max) = (Vec3::from(min), Vec3::from(max));
        self.world.add_box(min, max);
        self.pieces.push(Piece { min, max, kind });
    }
}

pub fn build() -> Level {
    let mut l = Level { world: BoxWorld::default(), pieces: Vec::new(), spawn: Vec3::new(0.0, 0.0, 0.0), spawn_yaw: 0.0 };

    // Ground with a hole for the pool at x -2600..-1400, y -2600..-1400.
    let (px0, px1, py0, py1) = (-2600.0, -1400.0, -2600.0, -1400.0);
    l.solid([-4000.0, -4000.0, -50.0], [px0, 4000.0, 0.0], Kind::Ground);
    l.solid([px1, -4000.0, -50.0], [8000.0, 4000.0, 0.0], Kind::Ground);
    l.solid([px0, -4000.0, -50.0], [px1, py0, 0.0], Kind::Ground);
    l.solid([px0, py1, -50.0], [px1, 4000.0, 0.0], Kind::Ground);
    // Pool basin: floor 400 down, water up to 20 below ground level.
    l.solid([px0, py0, -450.0], [px1, py1, -400.0], Kind::Ground);
    for (min, max) in [
        ([px0 - 50.0, py0, -450.0], [px0, py1, 0.0]),
        ([px1, py0, -450.0], [px1 + 50.0, py1, 0.0]),
        ([px0, py0 - 50.0, -450.0], [px1, py0, 0.0]),
        ([px0, py1, -450.0], [px1, py1 + 50.0, 0.0]),
    ] {
        l.solid(min, max, Kind::Wall);
    }
    let water = (Vec3::new(px0, py0, -400.0), Vec3::new(px1, py1, -20.0));
    l.world.add_water(water.0, water.1);
    l.pieces.push(Piece { min: water.0, max: water.1, kind: Kind::Water });

    // Mantle wall: blocks of rising height in front of spawn (step, low, medium, high, too high).
    for (i, h) in [30.0, 90.0, 150.0, 220.0, 320.0].into_iter().enumerate() {
        let y0 = -900.0 + i as f32 * 350.0;
        l.solid([800.0, y0, 0.0], [1100.0, y0 + 300.0, h], Kind::Ledge);
    }

    // Stairs (20 cm risers) up to a balcony.
    for i in 0..15 {
        let x0 = -400.0 + i as f32 * 40.0;
        l.solid([x0, 1300.0, 0.0], [x0 + 40.0, 1700.0, 20.0 * (i + 1) as f32], Kind::Stairs);
    }
    l.solid([200.0, 1300.0, 0.0], [800.0, 1700.0, 300.0], Kind::Ledge);

    // Sprint/slide lane with a crawl beam (only a crouched player fits under it).
    l.solid([1500.0, -2100.0, 100.0], [1700.0, -1500.0, 400.0], Kind::Wall);
    l.solid([1500.0, -2150.0, 0.0], [1700.0, -2100.0, 400.0], Kind::Wall);
    l.solid([1500.0, -1500.0, 0.0], [1700.0, -1450.0, 400.0], Kind::Wall);

    // Lean pillars.
    l.solid([-700.0, 600.0, 0.0], [-500.0, 800.0, 350.0], Kind::Wall);
    l.solid([-700.0, -800.0, 0.0], [-500.0, -600.0, 350.0], Kind::Wall);

    // Ladder tower and rooftops with gaps for Blink (tier I and tier II reach).
    let roof_h = 600.0;
    l.solid([2400.0, -300.0, 0.0], [2900.0, 300.0, roof_h], Kind::Roof);
    let ladder_min = Vec3::new(2380.0, -60.0, 0.0);
    let ladder_max = Vec3::new(2400.0, 60.0, roof_h);
    l.world.add_ladder(ladder_min, ladder_max, Vec3::new(-1.0, 0.0, 0.0));
    l.pieces.push(Piece { min: ladder_min, max: ladder_max, kind: Kind::Ladder });
    l.solid([3700.0, -300.0, 0.0], [4200.0, 300.0, roof_h + 100.0], Kind::Roof); // gap 800
    l.solid([5200.0, -300.0, 0.0], [5700.0, 300.0, roof_h + 200.0], Kind::Roof); // gap 1000
    l.solid([7100.0, -300.0, 0.0], [7600.0, 300.0, roof_h], Kind::Roof); // gap 1400
    // A high perch only reachable by blinking upward.
    l.solid([4600.0, 900.0, 0.0], [4900.0, 1200.0, 1100.0], Kind::Roof);

    // Dummy guards to blink into (Blink stops at pawns).
    for (i, p) in [Vec3::new(1800.0, 600.0, 88.0), Vec3::new(2000.0, -700.0, 88.0)].into_iter().enumerate() {
        let half = Vec3::new(31.0, 31.0, 87.5);
        l.world.add_pawn(p, half, i as u32 + 1);
        l.pieces.push(Piece { min: p - half, max: p + half, kind: Kind::Guard });
    }
    l
}

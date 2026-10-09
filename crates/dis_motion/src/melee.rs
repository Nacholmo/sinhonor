//! The basic sword attack, following the behaviour specified in `NOTES.md` §5c: forehand and
//! backhand swings that chain into each other, a sneak attack when crouched, a killing blow when
//! the targeted character can't survive the hit, and a recoil when the blade strikes the world.
//! Each swing is paced by its animation: when the blade can hit, when another press is taken for
//! the next swing, when the next swing may start, and when the swing is over.
//!
//! The kit decides what is hit; the host applies the damage (characters are the host's, see
//! [`World::pawn`]).

use crate::{Vec3, World};

/// Half-height of the box swept for the blade (game constant).
pub const SWEEP_HALF_HEIGHT: f32 = 30.0;
/// Half-height of the box swept for the crosshair target (game constant).
pub const CROSSHAIR_HALF_HEIGHT: f32 = 10.0;

/// One swing animation and its timing (seconds into the swing, already at the clip's play rate).
#[derive(Clone, Debug)]
pub struct SwingAnim {
    pub name: String,
    /// When the blade can hit, and for how long.
    pub zone: Option<(f32, f32)>,
    /// From here, an attack press is kept for the next swing.
    pub chain_input: Option<f32>,
    /// From here, the next swing may start.
    pub interruptible: f32,
    /// The swing is over.
    pub exit: f32,
}

/// A swing and the recoils that replace it when the blade strikes the world.
#[derive(Clone, Debug)]
pub struct SwingSet {
    pub swing: SwingAnim,
    pub env_hit: Option<SwingAnim>,
    /// The recoil for a chained swing.
    pub env_hit_chain: Option<SwingAnim>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SwingKind {
    Forehand,
    Backhand,
    /// Crouched: always a forehand.
    Sneak,
    /// The target can't survive the blow: an unblockable finishing swing.
    KillingForehand,
    KillingBackhand,
}

impl SwingKind {
    pub const ALL: [SwingKind; 5] = [SwingKind::Forehand, SwingKind::Backhand, SwingKind::Sneak, SwingKind::KillingForehand, SwingKind::KillingBackhand];

    pub fn index(self) -> usize {
        self as usize
    }

    fn forehand(self) -> bool {
        matches!(self, SwingKind::Forehand | SwingKind::Sneak | SwingKind::KillingForehand)
    }
}

#[derive(Clone, Debug)]
pub struct MeleeTuning {
    /// Base reach of a swing.
    pub range: f32,
    /// How much of the forward-speed bonus applies to the reach.
    pub ray_scale_percent: f32,
    /// Forward speed above `min_speed_ray_scale` lengthens the reach by this much per unit of
    /// speed, up to `ray_speed_scale_max`.
    pub ray_speed_scale: f32,
    pub ray_speed_scale_max: f32,
    pub min_speed_ray_scale: f32,
    /// Half-width of the box swept for the blade.
    pub sweep_size: f32,
    /// Half-width of the box swept for the crosshair target.
    pub crosshair_size: f32,
    /// A swing started within this long of the last one is the next in the chain.
    pub chain_time: f32,
    pub damage: f32,
    /// Camera shake when the blade strikes the world (the game's strength value).
    pub env_hit_shake: f32,
    /// Variants of each swing, indexed by [`SwingKind::index`].
    pub swings: [Vec<SwingSet>; 5],
}

impl MeleeTuning {
    /// The reach: the base range, longer when moving forward fast.
    pub fn reach(&self, velocity: Vec3, forward: Vec3) -> f32 {
        let along = velocity.dot(forward);
        let speed = if along > self.min_speed_ray_scale {
            1.0 + ((along - self.min_speed_ray_scale) * self.ray_speed_scale).clamp(0.0, self.ray_speed_scale_max)
        } else {
            1.0
        };
        self.range * ((speed - 1.0) * self.ray_scale_percent + 1.0)
    }
}

/// A swing under way.
#[derive(Clone, Debug)]
pub struct SwingRun {
    pub kind: SwingKind,
    pub variant: usize,
    /// The animation playing (the swing, or its recoil).
    pub anim: SwingAnim,
    /// Seconds into `anim`.
    pub t: f32,
    pub chained: bool,
    pub recoiled: bool,
    struck: bool,
    queued: bool,
}

/// Sword state the host can read.
#[derive(Clone, Debug, Default)]
pub struct Melee {
    pub run: Option<SwingRun>,
    /// The character under the crosshair, in reach.
    pub target: Option<u32>,
    clock: f32,
    last_start: Option<f32>,
    last_forehand: bool,
    rng: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MeleeEvent {
    /// A swing started, playing `anim`.
    Swing { kind: SwingKind, anim: String },
    /// The blade struck a character. `killing` if the damage leaves it with no health.
    Hit { target: u32, damage: f32, killing: bool },
    /// The blade struck the world at `point`; the swing recoils into `anim`.
    EnvHit { point: Vec3, normal: Vec3, shake: f32, anim: Option<String> },
}

/// What the sword needs from the player each tick.
pub(crate) struct Swordsman {
    pub eye: Vec3,
    pub aim: Vec3,
    pub velocity: Vec3,
    pub crouched: bool,
    /// This tick's attack press.
    pub attack: bool,
    /// False while the player can't fight (climbing, swimming, blinking...), which also cuts a
    /// swing short.
    pub can_fight: bool,
}

impl Melee {
    fn pick(&mut self, n: usize) -> usize {
        // xorshift: the game picks a swing's variant at random.
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        if n == 0 {
            0
        } else {
            self.rng as usize % n
        }
    }

    /// The character under the crosshair: a narrow box swept along the view; if that meets the
    /// world first, a line along the view gets a second chance at a character.
    pub(crate) fn crosshair(t: &MeleeTuning, world: &dyn World, me: &Swordsman) -> Option<u32> {
        let end = me.eye + me.aim * t.reach(me.velocity, me.aim);
        let half = Vec3::new(t.crosshair_size, t.crosshair_size, CROSSHAIR_HALF_HEIGHT);
        let hit = world.sweep(me.eye, end, half)?;
        if hit.is_pawn {
            return Some(hit.actor);
        }
        world.sweep(me.eye, end, Vec3::ZERO).filter(|h| h.is_pawn).map(|h| h.actor)
    }

    /// Starts the next swing: forehand first, then backhand when chained, alternating.
    fn start(&mut self, t: &MeleeTuning, world: &dyn World, me: &Swordsman, queued: bool, ev: &mut Vec<MeleeEvent>) {
        let chain = !me.crouched && (queued || self.last_start.is_some_and(|s| self.clock - s < t.chain_time));
        let target = Self::crosshair(t, world, me);
        let kills = !me.crouched && target.and_then(|a| world.pawn(a)).is_some_and(|p| p.health <= t.damage);
        let kind = if !chain || !self.last_forehand {
            if kills {
                SwingKind::KillingForehand
            } else if me.crouched {
                SwingKind::Sneak
            } else {
                SwingKind::Forehand
            }
        } else if kills {
            SwingKind::KillingBackhand
        } else {
            SwingKind::Backhand
        };
        let sets = &t.swings[kind.index()];
        if sets.is_empty() {
            return;
        }
        let variant = self.pick(sets.len());
        let anim = sets[variant].swing.clone();
        self.last_forehand = kind.forehand();
        self.last_start = Some(self.clock);
        ev.push(MeleeEvent::Swing { kind, anim: anim.name.clone() });
        self.run = Some(SwingRun { kind, variant, anim, t: 0.0, chained: chain, recoiled: false, struck: false, queued: false });
    }

    /// Runs the sword for one tick.
    pub(crate) fn tick(&mut self, t: &MeleeTuning, world: &dyn World, me: &Swordsman, dt: f32, ev: &mut Vec<MeleeEvent>) {
        self.clock += dt;
        if self.rng == 0 {
            self.rng = 0x9E37_79B9;
        }
        if !me.can_fight {
            self.run = None;
            return;
        }
        if me.attack {
            match self.run.as_mut() {
                None => return self.start(t, world, me, false, ev),
                Some(run) if run.t >= run.anim.interruptible => return self.start(t, world, me, true, ev),
                Some(run) if run.anim.chain_input.is_some_and(|c| run.t >= c) => run.queued = true,
                Some(_) => {}
            }
        }
        let Some(run) = self.run.as_mut() else { return };
        run.t += dt;
        if run.queued && run.t >= run.anim.interruptible {
            return self.start(t, world, me, true, ev);
        }
        if let (Some((zone, len)), false) = (run.anim.zone, run.struck) {
            if run.t >= zone && run.t <= zone + len + dt {
                let end = me.eye + me.aim * t.reach(me.velocity, me.aim);
                let half = Vec3::new(t.sweep_size, t.sweep_size, SWEEP_HALF_HEIGHT);
                if let Some(hit) = world.sweep(me.eye, end, half) {
                    run.struck = true;
                    if hit.is_pawn {
                        let killing = world.pawn(hit.actor).is_some_and(|p| p.health <= t.damage);
                        ev.push(MeleeEvent::Hit { target: hit.actor, damage: t.damage, killing });
                    } else {
                        let set = &t.swings[run.kind.index()][run.variant];
                        let recoil = if run.chained { set.env_hit_chain.as_ref().or(set.env_hit.as_ref()) } else { set.env_hit.as_ref() };
                        ev.push(MeleeEvent::EnvHit { point: hit.location, normal: hit.normal, shake: t.env_hit_shake, anim: recoil.map(|a| a.name.clone()) });
                        if let Some(r) = recoil {
                            run.anim = r.clone();
                            run.t = 0.0;
                            run.recoiled = true;
                        }
                    }
                }
            }
        }
        if run.t >= run.anim.exit {
            self.run = None;
        }
    }
}

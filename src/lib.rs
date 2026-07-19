use bevy::prelude::*;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use swarm_engine_api::prelude::{
    API_VERSION, ActionDescriptor, BodyPart, ConfigFieldDescriptor, ConfigValueType,
    DESCRIPTOR_SCHEMA_VERSION, DamageType, PlayerId, PluginDescriptor, SystemDescriptor, TickPhase,
};
use swarm_engine_plugin_sdk::prelude::{
    ActionRegistrationError, ActionRegistry, DamageIntent, DamageIntentBuffer, DeathMark, Drone,
    HealIntent, HealIntentBuffer, Owner, PendingDamage, PendingHeal, PendingSpecialAttack,
    Position, SpecialAttackKind, Structure,
};
use swarm_engine_plugin_sdk::traits::SwarmPlugin;

#[derive(Component, Debug, Clone)]
pub struct Tower {
    pub range: u32,
    pub damage: u32,
    pub damage_type: DamageType,
}

#[derive(Resource, Debug, Clone)]
pub struct CombatConfig {
    pub pvp_enabled: bool,
    pub friendly_fire: bool,
    pub damage_multiplier_bp: u32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        Self {
            pvp_enabled: true,
            friendly_fire: false,
            damage_multiplier_bp: 10_000,
        }
    }
}

#[derive(Resource, Debug, Clone, Default)]
pub struct CombatRegistry {
    pub damage_types: HashSet<DamageType>,
    pub body_parts: HashSet<BodyPart>,
}

type TowerQueryItem<'w> = (Entity, &'w Position, Option<&'w Owner>, &'w Tower);
type TargetQueryItem<'w> = (
    Entity,
    &'w Position,
    Option<&'w Owner>,
    Option<&'w Drone>,
    Option<&'w Structure>,
);

#[derive(Resource, Debug, Clone, Default)]
pub struct ResolvedStatusIntents {
    pub entries: Vec<ResolvedStatusIntent>,
}

#[derive(Debug, Clone)]
pub struct ResolvedStatusIntent {
    pub kind: SpecialAttackKind,
    pub target: Entity,
    pub amount: u32,
}

#[derive(Component, Debug, Clone, Default)]
pub struct StatusState {
    pub effects: BTreeMap<SpecialAttackKind, u32>,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct CombatCoreModPlugin;

impl Plugin for CombatCoreModPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CombatConfig>()
            .init_resource::<ActionRegistry>()
            .init_resource::<CombatRegistry>()
            .init_resource::<PendingDamage>()
            .init_resource::<PendingHeal>()
            .init_resource::<DamageIntentBuffer>()
            .init_resource::<HealIntentBuffer>()
            .init_resource::<PendingSpecialAttack>()
            .init_resource::<ResolvedStatusIntents>()
            .add_systems(Startup, register_combat_core)
            .add_systems(
                Update,
                (
                    tower_auto_attack_system,
                    attack_system,
                    ranged_attack_system,
                    heal_system,
                    special_attack_reducer,
                    damage_application_system,
                )
                    .chain(),
            );
    }
}

impl SwarmPlugin for CombatCoreModPlugin {
    fn descriptor() -> PluginDescriptor {
        let system = |system_id: &str,
                      phase,
                      order,
                      reads: &[&str],
                      writes: &[&str],
                      produces_buffers: &[&str],
                      consumes_buffers: &[&str]| SystemDescriptor {
            system_id: format!("combat-core.{system_id}"),
            version: "0.1.0".to_string(),
            phase,
            order,
            reads: reads.iter().map(|value| (*value).to_string()).collect(),
            writes: writes.iter().map(|value| (*value).to_string()).collect(),
            produces_buffers: produces_buffers
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            consumes_buffers: consumes_buffers
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
            deterministic_iteration: vec!["Entity".to_string()],
        };

        PluginDescriptor {
            id: "combat-core".to_string(),
            version: "0.1.0".to_string(),
            api_version: API_VERSION.to_string(),
            dependencies: Vec::new(),
            config: vec![ConfigFieldDescriptor {
                key: "damage_multiplier".to_string(),
                value_type: ConfigValueType::FixedBasisPoints,
                default: 10_000_u32.into(),
                required: false,
                validator: None,
            }],
            systems: vec![
                system(
                    "register",
                    TickPhase::Startup,
                    0,
                    &[],
                    &["ActionRegistry", "CombatRegistry"],
                    &[],
                    &[],
                ),
                system(
                    "tower-auto-attack",
                    TickPhase::Update,
                    0,
                    &[
                        "Tower",
                        "Position",
                        "Owner",
                        "Drone",
                        "Structure",
                        "CombatConfig",
                    ],
                    &["PendingDamage"],
                    &["PendingDamage"],
                    &[],
                ),
                system(
                    "attack",
                    TickPhase::Update,
                    1,
                    &["PendingDamage", "CombatConfig"],
                    &["PendingDamage", "DamageIntentBuffer"],
                    &["DamageIntentBuffer"],
                    &["PendingDamage"],
                ),
                system(
                    "ranged-attack",
                    TickPhase::Update,
                    2,
                    &["PendingDamage", "CombatConfig"],
                    &["PendingDamage", "DamageIntentBuffer"],
                    &["DamageIntentBuffer"],
                    &["PendingDamage"],
                ),
                system(
                    "heal",
                    TickPhase::Update,
                    3,
                    &["PendingHeal"],
                    &["PendingHeal", "HealIntentBuffer"],
                    &["HealIntentBuffer"],
                    &["PendingHeal"],
                ),
                system(
                    "special-attack-reducer",
                    TickPhase::Update,
                    4,
                    &["PendingSpecialAttack"],
                    &["PendingSpecialAttack", "ResolvedStatusIntents"],
                    &["ResolvedStatusIntents"],
                    &["PendingSpecialAttack"],
                ),
                system(
                    "damage-application",
                    TickPhase::Update,
                    5,
                    &[
                        "DamageIntentBuffer",
                        "HealIntentBuffer",
                        "Drone",
                        "Structure",
                    ],
                    &[
                        "DamageIntentBuffer",
                        "HealIntentBuffer",
                        "Drone",
                        "Structure",
                        "DeathMark",
                    ],
                    &[],
                    &["DamageIntentBuffer", "HealIntentBuffer"],
                ),
            ],
            actions: [
                ("Attack", "attack"),
                ("RangedAttack", "ranged_attack"),
                ("Heal", "heal"),
            ]
            .into_iter()
            .map(|(action_type, handler)| ActionDescriptor {
                action_type: action_type.to_string(),
                handler: handler.to_string(),
                payload_schema: serde_json::json!({
                    "type": "object",
                    "additionalProperties": false
                }),
                command_phase: TickPhase::Command,
                output_buffer: Some(
                    match action_type {
                        "Heal" => "PendingHeal",
                        _ => "PendingDamage",
                    }
                    .to_string(),
                ),
            })
            .collect(),
            descriptor_schema_version: DESCRIPTOR_SCHEMA_VERSION.to_string(),
        }
    }
}

pub fn register_combat_core(
    mut actions: ResMut<ActionRegistry>,
    mut registry: ResMut<CombatRegistry>,
) {
    for (action_type, handler) in [
        ("Attack", "attack"),
        ("RangedAttack", "ranged_attack"),
        ("Heal", "heal"),
    ] {
        actions
            .register(action_type, handler)
            .unwrap_or_else(|error: ActionRegistrationError| panic!("{error}"));
    }
    registry.damage_types.extend([
        DamageType::Kinetic,
        DamageType::Thermal,
        DamageType::EMP,
        DamageType::Sonic,
        DamageType::Corrosive,
        DamageType::Psionic,
    ]);
    registry.body_parts.extend([
        BodyPart::Attack,
        BodyPart::RangedAttack,
        BodyPart::Heal,
        BodyPart::Tough,
    ]);
}

pub fn tower_auto_attack_system(
    mut pending: ResMut<PendingDamage>,
    towers: Query<TowerQueryItem<'_>>,
    targets: Query<TargetQueryItem<'_>>,
    config: Res<CombatConfig>,
) {
    let mut queued = Vec::new();
    for (tower_entity, tower_pos, tower_owner, tower) in &towers {
        let mut candidates: Vec<_> = targets
            .iter()
            .filter_map(|(target, pos, owner, drone, structure)| {
                if target == tower_entity || drone.is_none() && structure.is_none() {
                    return None;
                }
                if !can_damage(tower_owner.map(|o| o.0), owner.map(|o| o.0), &config) {
                    return None;
                }
                let distance = range(tower_pos, pos)?;
                (distance <= tower.range).then_some((distance, target))
            })
            .collect();
        candidates.sort_by_key(|(distance, entity)| (*distance, entity.to_bits()));
        if let Some((_, target)) = candidates.first() {
            queued.push((*target, tower.damage, tower.damage_type));
        }
    }
    for (target, amount, damage_type) in queued {
        pending.push(target, amount, damage_type.as_str());
    }
}

pub fn attack_system(
    mut pending: ResMut<PendingDamage>,
    mut intents: ResMut<DamageIntentBuffer>,
    config: Res<CombatConfig>,
) {
    let entries = std::mem::take(&mut pending.entries);
    for entry in entries {
        if entry.damage_type == DamageType::Kinetic.as_str() {
            intents.0.push(DamageIntent {
                source: 0,
                target: entry.target.to_bits(),
                amount: scale(entry.amount, config.damage_multiplier_bp),
                damage_type: DamageType::Kinetic,
            });
        } else {
            pending.entries.push(entry);
        }
    }
}

pub fn ranged_attack_system(
    mut pending: ResMut<PendingDamage>,
    mut intents: ResMut<DamageIntentBuffer>,
    config: Res<CombatConfig>,
) {
    let entries = std::mem::take(&mut pending.entries);
    for entry in entries {
        if let Some(damage_type) = damage_type_from_name(&entry.damage_type) {
            intents.0.push(DamageIntent {
                source: 0,
                target: entry.target.to_bits(),
                amount: scale(entry.amount, config.damage_multiplier_bp),
                damage_type,
            });
        } else {
            pending.entries.push(entry);
        }
    }
}

pub fn heal_system(mut pending: ResMut<PendingHeal>, mut intents: ResMut<HealIntentBuffer>) {
    intents
        .0
        .extend(pending.entries.drain(..).map(|entry| HealIntent {
            source: 0,
            target: entry.target.to_bits(),
            amount: entry.amount,
        }));
}

pub fn special_attack_reducer(
    mut input: ResMut<PendingSpecialAttack>,
    mut output: ResMut<ResolvedStatusIntents>,
) {
    let mut raw = std::mem::take(&mut input.intents);
    raw.sort_by(|a, b| {
        b.kind
            .cmp(&a.kind)
            .then_with(|| a.source.to_bits().cmp(&b.source.to_bits()))
            .then_with(|| a.target.to_bits().cmp(&b.target.to_bits()))
    });
    let mut seen = BTreeSet::new();
    output.entries = raw
        .into_iter()
        .filter(|intent| seen.insert(intent.target))
        .map(|intent| ResolvedStatusIntent {
            kind: intent.kind,
            target: intent.target,
            amount: intent.amount,
        })
        .collect();
}

pub fn damage_application_system(
    mut commands: Commands,
    mut damage: ResMut<DamageIntentBuffer>,
    mut heal: ResMut<HealIntentBuffer>,
    mut drones: Query<&mut Drone>,
    mut structures: Query<&mut Structure, Without<Drone>>,
) {
    let mut damage_by_target: HashMap<Entity, u32> = HashMap::new();
    for entry in damage.0.drain(..) {
        let target = Entity::from_bits(entry.target);
        let reduced = match entry.damage_type {
            DamageType::Kinetic => entry.amount,
            _ => entry.amount,
        };
        *damage_by_target.entry(target).or_default() = damage_by_target
            .get(&target)
            .copied()
            .unwrap_or(0)
            .saturating_add(reduced);
    }

    let mut heal_by_target: HashMap<Entity, u32> = HashMap::new();
    for entry in heal.0.drain(..) {
        let target = Entity::from_bits(entry.target);
        *heal_by_target.entry(target).or_default() = heal_by_target
            .get(&target)
            .copied()
            .unwrap_or(0)
            .saturating_add(entry.amount);
    }

    let mut targets: Vec<_> = damage_by_target
        .keys()
        .chain(heal_by_target.keys())
        .copied()
        .collect();
    targets.sort_by_key(|entity| entity.to_bits());
    targets.dedup();
    for target in targets {
        let damage = damage_by_target.get(&target).copied().unwrap_or(0);
        let heal = heal_by_target.get(&target).copied().unwrap_or(0);
        if let Ok(mut drone) = drones.get_mut(target) {
            drone.hits = drone
                .hits
                .saturating_sub(damage)
                .saturating_add(heal)
                .min(drone.hits_max);
            if drone.hits == 0 {
                commands.entity(target).insert(DeathMark);
            }
        } else if let Ok(mut structure) = structures.get_mut(target) {
            structure.hits = structure
                .hits
                .saturating_sub(damage)
                .saturating_add(heal)
                .min(structure.hits_max);
            if structure.hits == 0 {
                commands.entity(target).insert(DeathMark);
            }
        }
    }
}

fn range(a: &Position, b: &Position) -> Option<u32> {
    (a.room == b.room).then(|| a.x.abs_diff(b.x).max(a.y.abs_diff(b.y)))
}

fn can_damage(source: Option<PlayerId>, target: Option<PlayerId>, config: &CombatConfig) -> bool {
    if source.is_some() && target.is_some() && source != target {
        return config.pvp_enabled;
    }
    source != target || config.friendly_fire
}

fn damage_type_from_name(name: &str) -> Option<DamageType> {
    [
        DamageType::Kinetic,
        DamageType::Thermal,
        DamageType::EMP,
        DamageType::Sonic,
        DamageType::Corrosive,
        DamageType::Psionic,
    ]
    .into_iter()
    .find(|damage_type| damage_type.as_str() == name)
}

fn scale(amount: u32, multiplier_bp: u32) -> u32 {
    ((amount as u64 * multiplier_bp as u64) / 10_000).min(u32::MAX as u64) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_applies_basis_point_multiplier() {
        assert_eq!(scale(200, 5_000), 100);
        assert_eq!(scale(200, 10_000), 200);
        assert_eq!(scale(200, 15_000), 300);
    }

    #[test]
    fn combat_config_blocks_friendly_fire_by_default() {
        let config = CombatConfig::default();

        assert!(can_damage(Some(1), Some(2), &config));
        assert!(!can_damage(Some(1), Some(1), &config));
    }

    #[test]
    fn descriptor_is_valid_and_identifies_combat_core() {
        let descriptor = CombatCoreModPlugin::descriptor();
        swarm_engine_api::validation::assert_valid_descriptor(&descriptor);
        assert_eq!(descriptor.id, "combat-core");
        assert_eq!(descriptor.version, "0.1.0");
        assert!(descriptor.dependencies.is_empty());
        assert_eq!(descriptor.systems.len(), 7);
        assert!(
            descriptor
                .systems
                .iter()
                .any(|system| system.system_id == "combat-core.damage-application")
        );
    }

    #[test]
    fn plugin_uses_engine_canonical_resources_and_buffers() {
        use std::any::TypeId;

        assert_eq!(
            TypeId::of::<ActionRegistry>(),
            TypeId::of::<swarm_engine_plugin_sdk::resources::ActionRegistry>()
        );
        assert_eq!(
            TypeId::of::<PendingDamage>(),
            TypeId::of::<swarm_engine_plugin_sdk::buffers::PendingDamage>()
        );
        assert_eq!(
            TypeId::of::<DamageIntentBuffer>(),
            TypeId::of::<swarm_engine_plugin_sdk::buffers::DamageIntentBuffer>()
        );
        assert_eq!(
            TypeId::of::<PendingSpecialAttack>(),
            TypeId::of::<swarm_engine_plugin_sdk::buffers::PendingSpecialAttack>()
        );

        let mut app = App::new();
        let target = app
            .world_mut()
            .spawn(Drone {
                owner: 7,
                body: Vec::new(),
                carry: Default::default(),
                carry_capacity: 0,
                fatigue: 0,
                hits: 100,
                hits_max: 100,
                spawning: false,
                age: 0,
                last_action_tick: 0,
                lifespan: 100,
            })
            .id();

        let mut actions = ActionRegistry::default();
        actions.register("EngineAction", "engine_action").unwrap();
        let mut pending_damage = PendingDamage::default();
        pending_damage.push(target, 5, DamageType::Kinetic.as_str());
        let mut pending_heal = PendingHeal::default();
        pending_heal.push(target, 2);
        let pending_special = PendingSpecialAttack {
            intents: vec![swarm_engine_plugin_sdk::prelude::StatusActionIntent {
                kind: SpecialAttackKind::Hack,
                source: target,
                target,
                owner: 7,
                amount: 4,
            }],
        };

        app.insert_resource(actions)
            .insert_resource(pending_damage)
            .insert_resource(pending_heal)
            .insert_resource(pending_special)
            .insert_resource(DamageIntentBuffer(vec![DamageIntent {
                source: 99,
                target: target.to_bits(),
                amount: 10,
                damage_type: DamageType::Thermal,
            }]))
            .insert_resource(HealIntentBuffer(vec![HealIntent {
                source: 99,
                target: target.to_bits(),
                amount: 3,
            }]))
            .add_plugins(CombatCoreModPlugin);

        app.update();

        let world = app.world();
        let actions = world.resource::<ActionRegistry>();
        assert_eq!(
            actions.handlers.get("EngineAction").map(String::as_str),
            Some("engine_action")
        );
        assert_eq!(
            actions.handlers.get("Attack").map(String::as_str),
            Some("attack")
        );
        assert!(world.resource::<PendingDamage>().entries.is_empty());
        assert!(world.resource::<PendingHeal>().entries.is_empty());
        assert!(world.resource::<DamageIntentBuffer>().0.is_empty());
        assert!(world.resource::<HealIntentBuffer>().0.is_empty());
        assert!(world.resource::<PendingSpecialAttack>().intents.is_empty());
        assert_eq!(world.get::<Drone>(target).unwrap().hits, 90);
        assert_eq!(world.resource::<ResolvedStatusIntents>().entries.len(), 1);
    }
}

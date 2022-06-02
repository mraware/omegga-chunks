use std::{collections::HashMap, fs::File, sync::Arc, time::Duration};

use anyhow::{anyhow, Result};
use brickadia::{
    read::SaveReader,
    save::{Brick, BrickColor, BrickOwner, Color, Header2, SaveData, Size},
};
use lazy_static::lazy_static;
use omegga::{Omegga, events::Event};
use serde::{Deserialize, Serialize};
use serde_json::{json};
use tokio::{sync::RwLock, time::sleep};

const SAVE_NAME: &'static str = "_omegga_chunks";
const MARKER_OWNER_UUID: &'static str = "00000000-0000-0000-0000-000000000001";
const CHUNK_SIZE: i32 = 512;
const COLLIDER_LIMIT: u32 = 65000;
const COMPONENT_LIMIT: u32 = 75;
const MARKER_COLORS: [BrickColor; 5] = [
    BrickColor::Unique(Color {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    }),
    BrickColor::Unique(Color {
        r: 0,
        g: 255,
        b: 0,
        a: 255,
    }),
    BrickColor::Unique(Color {
        r: 255,
        g: 0,
        b: 0,
        a: 255,
    }),
    BrickColor::Unique(Color {
        r: 0,
        g: 0,
        b: 255,
        a: 255,
    }),
      BrickColor::Unique(Color {
        r: 255,
        g: 0,
        b: 255,
        a: 255,
    })
];

#[rustfmt::skip]
const CHUNK_CORNERS: [(i32, i32, i32); 8] = [
    (-CHUNK_SIZE / 2 + 1, -CHUNK_SIZE / 2 + 1, -CHUNK_SIZE / 2 + 1),
    ( CHUNK_SIZE / 2 - 1, -CHUNK_SIZE / 2 + 1, -CHUNK_SIZE / 2 + 1),
    (-CHUNK_SIZE / 2 + 1,  CHUNK_SIZE / 2 - 1, -CHUNK_SIZE / 2 + 1),
    ( CHUNK_SIZE / 2 - 1,  CHUNK_SIZE / 2 - 1, -CHUNK_SIZE / 2 + 1),
    (-CHUNK_SIZE / 2 + 1, -CHUNK_SIZE / 2 + 1,  CHUNK_SIZE / 2 - 1),
    ( CHUNK_SIZE / 2 - 1, -CHUNK_SIZE / 2 + 1,  CHUNK_SIZE / 2 - 1),
    (-CHUNK_SIZE / 2 + 1,  CHUNK_SIZE / 2 - 1,  CHUNK_SIZE / 2 - 1),
    ( CHUNK_SIZE / 2 - 1,  CHUNK_SIZE / 2 - 1,  CHUNK_SIZE / 2 - 1),
];

pub fn pos_to_chunk(pos: (i32, i32, i32)) -> (i32, i32, i32) {
    fn round(n: i32) -> i32 {
        (n as f64 / CHUNK_SIZE as f64).floor() as i32
    }

    (round(pos.0), round(pos.1), round(pos.2))
}

pub fn chunk_center(pos: (i32, i32, i32)) -> (i32, i32, i32) {
    (
        CHUNK_SIZE / 2 + pos.0 * CHUNK_SIZE,
        CHUNK_SIZE / 2 + pos.1 * CHUNK_SIZE,
        CHUNK_SIZE / 2 + pos.2 * CHUNK_SIZE,
    )
}

pub fn chunk_corner(i: usize, center: (i32, i32, i32)) -> (i32, i32, i32) {
    (
        center.0 + CHUNK_CORNERS[i].0,
        center.1 + CHUNK_CORNERS[i].1,
        center.2 + CHUNK_CORNERS[i].2,
    )
}

struct AnalyzedSave {
    chunk_colliders: HashMap<(i32, i32, i32), (u32, u32, u32)>,
}

impl From<SaveData> for AnalyzedSave {
    fn from(data: SaveData) -> Self {
        lazy_static! {
            static ref BRICK_COLLIDERS: HashMap<String, u32> =
                serde_json::from_reader(File::open("colliders.json").unwrap()).unwrap();
        }

        let mut map = HashMap::new();
        for brick in data.bricks.into_iter() {
            let chunk_pos = pos_to_chunk(brick.position);
            let collider_count = *BRICK_COLLIDERS
                .get(data.header2.brick_assets[brick.asset_name_index as usize].as_str())
                .unwrap_or(&1);
            let component_count = brick.components.keys().len() as u32;
            map.entry(chunk_pos)
                .and_modify(|c: &mut (u32, u32, u32)| *c = (c.0 + 1, c.1 + collider_count, c.2 + component_count))
                .or_insert((1, collider_count, component_count));
        }
        Self {
            chunk_colliders: map,
        }
    }
}

pub fn mark_chunks(chunks: &[((i32, i32, i32), Option<(u32, u32, u32)>)]) -> SaveData {
    let mut bricks = vec![];

    for (pos, opt) in chunks.iter() {
        let center = chunk_center(*pos);
        let col = match opt {
            Some((_, colliders, components)) if *colliders > COLLIDER_LIMIT && *components > COMPONENT_LIMIT => 4,
            Some((_, _colliders, components)) if *components > COMPONENT_LIMIT => 3,
            Some((_, colliders, _components)) if *colliders > COLLIDER_LIMIT => 2,
            Some((_, colliders, _components)) if *colliders <= COLLIDER_LIMIT => 1,
            _ => 0,
        };

        for i in 0..8 {
            bricks.push(Brick {
                owner_index: 1,
                asset_name_index: 0,
                material_index: 0,
                material_intensity: 5,
                color: MARKER_COLORS[col].clone(),
                size: Size::Procedural(1, 1, 1),
                position: chunk_corner(i, center),
                ..Default::default()
            })
        }
    }

    SaveData {
        header2: Header2 {
            brick_assets: vec!["PB_DefaultMicroBrick".into()],
            materials: vec!["BMC_Glow".into()],
            brick_owners: vec![BrickOwner {
                id: MARKER_OWNER_UUID.parse().unwrap(),
                name: "Chunk Marker".into(),
                bricks: 0,
            }],
            ..Default::default()
        },
        bricks,
        ..Default::default()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct AuthUser {
    name: String,
    id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Config {
    authorized: Vec<AuthUser>,
}

#[tokio::main]
async fn main() {
    let omegga = Arc::new(Omegga::new());
    let mut rx = omegga.spawn();

    let analyzed_save: Arc<RwLock<Option<AnalyzedSave>>> = Arc::new(RwLock::new(None));
    let config: Arc<RwLock<Option<Config>>> = Arc::new(RwLock::new(None));

    while let Some(message) = rx.recv().await {
        match message {
            Event::Init { id, config: _config } =>
            {
              let mut cfg = config.write().await;
              *cfg = serde_json::from_value(_config).unwrap();
              omegga.write_response(
                  id,
                  Some(json!({"registeredCommands": ["chunks"]})),
                  None,
              );
            }
            Event::Stop { id } => {
              omegga.write_response(
                id,
                None,
                None,
              );
            }
            Event::Command { player, command, args } => {
              if command == "chunks" {
                let omegga = omegga.clone();
                let config = config.clone();
                let analyzed_save = analyzed_save.clone();
  
                tokio::spawn(async move {
                    if let Err(e) =
                        run_command(omegga.clone(), config, analyzed_save, player, args).await
                    {
                        omegga.error(format!("An error occurred: {}", e));
                    }
                });
              }
            }
            _ => (),
        }
    }
}

async fn run_command(
    omegga: Arc<Omegga>,
    config: Arc<RwLock<Option<Config>>>,
    analyzed_save: Arc<RwLock<Option<AnalyzedSave>>>,
    user: String,
    args: Vec<String>,
) -> Result<()> {
    let config_read = config.read().await;
    let config = match &*config_read {
        Some(cfg) => cfg,
        None => return Ok(()),
    };

    let command = &args[0];

    if !config
        .authorized
        .iter()
        .any(|u| u.name.to_lowercase() == user.to_lowercase())
    {
        omegga.whisper(
            user,
            "<color=\"a00\">You are not authorized to use this command!</>",
        );
        return Ok(());
    }

    match command.as_str() {
        "analyze" => {
            // save and get the save's path
            if let Err(_) = omegga.save_bricks(SAVE_NAME).await {
                omegga.whisper(user, "<color=\"a00\">Failed to save!</>");
                return Ok(());
            }
            sleep(Duration::from_millis(2500)).await;
            let path = match omegga.get_save_path(SAVE_NAME).await {
                Ok(Some(p)) => p,
                _ => {
                    omegga.whisper(user, "<color=\"a00\">Failed to find save! Try again.</>");
                    return Ok(());
                }
            };

            // read the save (we can't use tokio for this)
            let data = SaveReader::new(File::open(path).unwrap())
                .unwrap()
                .read_all_skip_preview()
                .unwrap();

            // set the analyzed save
            analyzed_save.write().await.replace(data.into());

            omegga.whisper(user, "<color=\"0a0\">The save has been analyzed. Any subsequent changes must be reanalyzed.</>");
        }
        "in" => {
            // find the chunk the current player is in
            let pos = omegga
                .get_player_position(user.clone())
                .await?
                .ok_or(anyhow!("player has no position"))?;
            omegga.whisper(
                user,
                format!(
                    "You are in chunk {:?}.",
                    pos_to_chunk((pos.0 as i32, pos.1 as i32, pos.2 as i32))
                ),
            );
        }
        "count" => {
            // list the bricks/colliders in this chunk
            match &*analyzed_save.read().await {
                Some(save) => {
                    let pos = omegga.get_player_position(user.clone()).await?.ok_or(anyhow!("player has no position"))?;

                    let chunk_pos = pos_to_chunk((pos.0 as i32, pos.1 as i32, pos.2 as i32));
                    if let Some((bricks, colliders, components)) = save.chunk_colliders.get(&chunk_pos) {
                        omegga.whisper(user, format!(
                            "There are <b>{} bricks</>, <b><color=\"{}\">{} colliders</></>, and <b>{} components</> in the chunk {:?}.",
                            bricks,
                            if *colliders > COLLIDER_LIMIT { "a00" } else { "0a0" },
                            colliders,
                            components,
                            chunk_pos,
                        ));
                    } else {
                        omegga.whisper(user, "<color=\"a00\">This chunk has no bricks or colliders!</>");
                    }
                }
                None => omegga.whisper(user, "<color=\"a00\">The save has not been analyzed! Analyze it first with <code>/chunks analyze</>.</>"),
            }
        }
        "mark" => {
            // mark the chunk we're currently in
            match &*analyzed_save.read().await {
                Some(save) => {
                    let pos = omegga.get_player_position(user.clone()).await?.ok_or(anyhow!("player has no position"))?;
                    let chunk_pos = pos_to_chunk((pos.0 as i32, pos.1 as i32, pos.2 as i32));
                    let opt = save.chunk_colliders.get(&chunk_pos);
                    let marker_data = mark_chunks(&vec![(chunk_pos, opt.copied())]);
                    omegga.load_save_data(marker_data, true, (0, 0, 0)).await?;
                    omegga.whisper(user, "<color=\"0a0\">Your chunk has been marked.</>");
                }
                None => omegga.whisper(user, "<color=\"a00\">The save has not been analyzed! Analyze it first with <code>/chunks analyze</>.</>"),
            }
        }
        "markall" => {
            // mark the chunk we're currently in
            match &*analyzed_save.read().await {
                Some(save) => {
                    let mut chunks = vec![];
                    for (pos, opt) in save.chunk_colliders.iter() {
                        chunks.push((*pos, Some(*opt)));
                    }
                    let marker_data = mark_chunks(&chunks);
                    omegga.load_save_data(marker_data, true, (0, 0, 0)).await?;
                    omegga.whisper(user, "<color=\"0a0\">All chunks have been marked.</>");
                }
                None => omegga.whisper(user, "<color=\"a00\">The save has not been analyzed! Analyze it first with <code>/chunks analyze</>.</>"),
            }
        }
        "clear" => {
            // clear chunk markers
            omegga.clear_bricks(MARKER_OWNER_UUID, true);
            omegga.whisper(user, "<color=\"0a0\">Chunk markers have been cleared.</>");
        }
        unknown => omegga.whisper(user, format!("Unknown subcommand {}.", unknown)),
    }

    Ok(())
}

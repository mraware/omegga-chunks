# omegga-chunks

This is a Rust port of [omegga-chunk-analyzer](https://github.com/voximity/omegga-chunk-analyzer), a plugin for
analyzing chunks in-game by the number of bricks in them and their number of colliders. Useful for when your
maps become so dense that you run into collision issues.

## Installation

`omegga install gh:voximity/omegga-chunks`

## Usage

| **Command** | **Description** |
| --- | --- |
| `/chunks in` | Gets the coordinate of the chunk you are currently in. |
| `/chunks analyze` | Analyze the chunks in the current save. Necessary to be ran before any command below this one. |
| `/chunks count` | Count the number of bricks and colliders in the chunk you're in. |
| `/chunks mark` | Place markers at the eight corners of the chunk you're in. White means the chunk has no bricks, green means the collider count is below max (65,000), and red means the collider count exceeds the limit. |
| `/chunks markall` | Place markers at the eight corners of every chunk with bricks. See above for color codes. |
| `/chunks clear` | Clear all chunk markers, if any. |

## Credits

* voximity - creator, maintainer
* Meshiest - Omegga, collider test procedure

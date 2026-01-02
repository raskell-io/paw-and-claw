# Paw & Claw Sprite Prompts

AI-assisted sprite generation prompts for PixelLab.ai and similar tools.

## Game Theme

**"Paw & Claw"** - Woodland creature tactical warfare (Advance Wars meets Redwall)

### Factions

| Faction | Theme | Animals | Accent Color |
|---------|-------|---------|--------------|
| Eastern Empire | Asian woodland | Tanuki, foxes, red pandas | Warm red `#E85D4C` |
| Northern Realm | Arctic/boreal | Bears, wolves, owls | Ice blue `#5B8DBE` |
| Western Frontier | American West | Raccoons, hawks, coyotes | Sage green `#8B9556` |
| Southern Pride | African savanna | Lions, elephants, cheetahs | Golden amber `#E6B84D` |

### Art Style

- Charming pixel art with warm, natural colors
- Top-down tactical perspective
- Medieval/fantasy woodland aesthetic
- Cozy but capable of depicting warfare

### Recommended Sizes

- **Terrain features**: 32x32 pixels
- **Units**: 24x24 pixels
- **UI elements**: 16x16 pixels

## Directory Structure

```
prompts/
├── README.md           # This file
├── art-direction.md    # Full art direction document
├── palette.md          # Color palette reference
├── terrain/            # Terrain feature prompts
│   ├── trees.md
│   ├── rocks.md
│   ├── water.md
│   └── buildings.md
├── units/              # Unit prompts by faction
│   ├── eastern/
│   ├── northern/
│   ├── western/
│   └── southern/
└── ui/                 # UI element prompts
```

## Usage Tips

1. Generate at 2x or 4x target size, then downscale for cleaner results
2. Request "transparent background" for game sprites
3. Add "top-down tactical game" for consistent perspective
4. Include faction color accents for unit consistency

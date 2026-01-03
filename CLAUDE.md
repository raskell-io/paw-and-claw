# Paw & Claw - Project Context

## Overview
Turn-based tactics game inspired by Advance Wars, built with Bevy 0.15 (Rust).

## Tech Stack
- **Engine**: Bevy 0.15
- **Renderer**: wgpu (default, WebGPU-based)
- **Platforms**: Windows, macOS, Linux (Web planned)

## Architecture
- 3D board with 2D billboarded sprites (Advance Wars: Dual Strike style)
- ECS-based game logic
- Procedural graphics (no external sprite assets yet)

---

## Portability Goal

Keep codebase clean and platform-agnostic. Console (Switch) is a future possibility but not a "flip a switch" workflow - requires porting work under Nintendo's developer program.

**Current focus**: Write portable code now, worry about console later.

See `.claude/portability.md` for guidelines.

### Key Principles
1. **Gamepad-first UX** - All gameplay works with controller
2. **Conservative GPU** - Basic 3D, StandardMaterial, billboards
3. **No platform hacks** - Use Bevy's abstractions
4. **Resolution flexible** - UI scales to various screens

# Portability Guidelines

Keep codebase clean and platform-agnostic while using Bevy as daily driver.

## Design Principles

1. **Gamepad-first UX** - All gameplay must work with controller only
2. **No platform-specific code** - Use Bevy's abstractions
3. **Conservative GPU features** - Stick to StandardMaterial, basic 3D
4. **Resolution flexibility** - UI must scale to various screen sizes

## Input Guidelines

- Primary: Gamepad/keyboard navigation
- Mouse: Supported but never required
- All menus navigable with D-pad
- Confirm = A/Space/Enter, Cancel = B/Escape

## Graphics Guidelines

- Use Bevy's built-in materials (StandardMaterial)
- Avoid custom shaders unless necessary
- Keep texture sizes reasonable
- Billboard sprites are portable and efficient

## File I/O Guidelines

- Use Bevy's asset system exclusively
- No hardcoded file paths
- Save data through Bevy-compatible means

## Testing

- Regularly test with gamepad connected
- Consider Steam Deck as a portability proxy

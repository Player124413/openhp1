# OpenHP1 settings

OpenHP1 stores its user-editable settings in `OpenHP1.ini`. The file is created
automatically the first time you launch the game, so you do not need to create
it yourself.

## File location

| Operating system | Default location |
| --- | --- |
| Windows | `%APPDATA%\OpenHP1\OpenHP1.ini` |
| macOS | `~/Library/Application Support/OpenHP1/OpenHP1.ini` |
| Linux and other Unix systems | `$XDG_CONFIG_HOME/openhp1/OpenHP1.ini`, or `~/.config/openhp1/OpenHP1.ini` when `XDG_CONFIG_HOME` is not set |
| Android | `/sdcard/Android/data/org.openhp1.game/files/OpenHP1.ini` or `/sdcard/OpenHP1/OpenHP1.ini` |

If `OPENHP1_SETTINGS_DIR` is set, OpenHP1 uses that directory instead. This is
mainly useful for portable installations and troubleshooting.

Close OpenHP1 before editing the file. Your changes are read the next time the
game starts. Section names, key names, and named values are not case-sensitive,
so `XeGTAO`, `xegtao`, and `XeGtAo` all mean the same thing.

## Default file

A newly generated file looks like this:

```ini
[OpenHP1.GameData]
Root=/path/to/Harry Potter TM
Language=eng

[OpenHP1.Gameplay]
SkipIntro=false
JumpSkipsCutscenes=false
AutoLearnSpells=false
InstantPickupWizardCards=false

[OpenHP1.Renderer]
ResolutionX=1024
ResolutionY=768
WindowSizeX=1280
WindowSizeY=800
Renderer=Classic
DetailTextures=false
Etc2Compression=false

[OpenHP1.Touch]
Enabled=true
Opacity=0.75
LookSensitivity=1.0
StickX=0.150
StickY=0.720
StickRadius=64.0
StickVisible=true
CastX=0.880
CastY=0.720
CastSize=46.0
CastVisible=true
JumpX=0.770
JumpY=0.820
JumpSize=40.0
JumpVisible=true
InteractX=0.880
InteractY=0.520
InteractSize=36.0
InteractVisible=true
SneakX=0.150
SneakY=0.420
SneakSize=32.0
SneakVisible=true
BroomBoostX=0.760
BroomBoostY=0.630
BroomBoostSize=34.0
BroomBoostVisible=true
BroomBrakeX=0.760
BroomBrakeY=0.470
BroomBrakeSize=34.0
BroomBrakeVisible=true
MenuX=0.940
MenuY=0.080
MenuSize=26.0
MenuVisible=true
ConsoleX=0.060
ConsoleY=0.080
ConsoleSize=26.0
ConsoleVisible=true

[WinDrv.WindowsClient]
ScreenFlashes=true

[OpenHP1.Renderer.Classic]
Brightness=0.5
ColorMode=32Bit
CRTEffect=false

[OpenHP1.Renderer.Modern]
ToneMapper=Reinhard
ReinhardBrightness=0.66
ReinhardContrast=1.05
ACESBrightness=0.64
ACESContrast=0.75
AgXBrightness=0.6
AgXContrast=0.9
AmbientOcclusion=XeGTAO
AntiAliasing=SMAA
Bloom=false
VolumetricLighting=false
```

## `[OpenHP1.GameData]`

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `Root` | Automatically detected when possible | An absolute directory path | Selects the original game folder containing `Maps` and `System`. |
| `Language` | The first language shipped with the selected game files | A language code offered by the launcher, such as `eng`, `fre`, or `ger` | Selects the original game's localized text, speech, and textures. |

The launcher discovers languages from the selected game files and validates both
values before writing them. When `Root` is absent, OpenHP1 checks for a local
`res` directory and the standard Windows installation directory. If the saved
root is no longer valid, OpenHP1 tries those locations again and remembers the
first valid replacement. It reports the saved-root error only when auto-detection
also fails.

## `[OpenHP1.Gameplay]`

These optional tweaks are disabled by default, preserving the authored game
flow.

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `SkipIntro` | `false` | `true`, `false` | Starts a new game directly in `Lev_Tut1` instead of playing the opening storybook sequence. |
| `JumpSkipsCutscenes` | `false` | `true`, `false` | Makes any jump input fast-forward the active cutscene through its shipped `CutSkip` path until the cutscene and final camera transition finish. |
| `AutoLearnSpells` | `false` | `true`, `false` | Completes triggered spell-learning sequences immediately, learns the spell, and awards the sum of all four authored round scores. |
| `InstantPickupWizardCards` | `false` | `true`, `false` | Collects wizard cards immediately on touch without playing Harry's card-pickup animation or the card's rising effect. |

These settings also accept `1`, `on`, `0`, and `off`.

## `[OpenHP1.Renderer]`

These settings choose the game resolution, initial window size, and render
pipeline.

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `ResolutionX` | `1024` | `320` to `8192` | Sets the width of the internally rendered game image. |
| `ResolutionY` | `768` | `320` to `8192` | Sets the height of the internally rendered game image. |
| `WindowSizeX` | `1280` | `320` to `8192` | Sets the width of the window when OpenHP1 starts. The window remains resizable. |
| `WindowSizeY` | `800` | `320` to `8192` | Sets the height of the window when OpenHP1 starts. The window remains resizable. |
| `Renderer` | `Classic` | `Classic`, `Modern` | Chooses the original-style or enhanced render pipeline. |
| `DetailTextures` | `false` | `true`, `false` | Enables the original three-band close-range detail-texture overlay in both renderers. Macro textures remain enabled independently. |
| `Etc2Compression` | `true` on Android, `false` otherwise | `true`, `false` | Enables hardware ETC2/EAC texture compression on supported GPUs, cutting texture memory by up to 8x and boosting mobile performance. |

Both values in a width and height pair must be valid. OpenHP1 limits each pair
to no more total pixels than 3840x2160. If a pair is incomplete, invalid, or too
large, OpenHP1 uses its default instead.

`ResolutionX` and `ResolutionY` control the game image, not the physical window.
This lets you keep the original 1024x768 presentation inside a larger window.

## `[WinDrv.WindowsClient]`

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `ScreenFlashes` | `true` | `true`, `false` | Shows authored viewport flashes and fades. When false, their runtime timing continues but the rendered image is unchanged. |

`ScreenFlashes` also accepts `1`, `on`, `0`, and `off`.

## `[OpenHP1.Renderer.Classic]`

These settings apply when `Renderer=Classic`.

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `Brightness` | `0.5` | `0.2` to `1.0` | Adjusts the image from darker to brighter. Values outside this range are moved to the nearest limit. |
| `ColorMode` | `32Bit` | `32Bit`, `RGB565` | Uses full colour or emulates the original 16-bit RGB565 output. |
| `CRTEffect` | `false` | `true`, `false` | Enables a sharp late-1990s/early-2000s PC CRT presentation with fine scanlines, a subtle aperture grille, mild curvature and vignette, and highlight halation. |

For compatibility, `ColorMode` also accepts `32`, `TrueColor`, and `RGBA8888`
as names for `32Bit`, and `16` or `16Bit` as names for `RGB565`. OpenHP1 writes
the preferred `32Bit` or `RGB565` spelling back to the file.

`CRTEffect` also accepts `1`, `on`, `0`, and `off`. It is disabled by default
and has no effect when the Modern renderer is selected.

## `[OpenHP1.Renderer.Modern]`

These settings apply when `Renderer=Modern`.

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `ToneMapper` | `Reinhard` | `AgX`, `Reinhard`, `ACES` | Chooses how the Modern renderer turns its brighter image into colours your display can show. |
| `ReinhardBrightness` | `0.66` | `0.2` to `1.0` | Sets brightness when using Reinhard. Values outside this range are moved to the nearest limit. |
| `ReinhardContrast` | `1.05` | `0.5` to `2.0` | Sets contrast when using Reinhard. Values outside this range are moved to the nearest limit. |
| `ACESBrightness` | `0.64` | `0.2` to `1.0` | Sets brightness when using ACES. Values outside this range are moved to the nearest limit. |
| `ACESContrast` | `0.75` | `0.5` to `2.0` | Sets contrast when using ACES. Values outside this range are moved to the nearest limit. |
| `AgXBrightness` | `0.6` | `0.2` to `1.0` | Sets brightness when using AgX. Values outside this range are moved to the nearest limit. |
| `AgXContrast` | `0.9` | `0.5` to `2.0` | Sets contrast when using AgX. Values outside this range are moved to the nearest limit. |
| `AmbientOcclusion` | `XeGTAO` | `Off`, `SSAO`, `XeGTAO` | Chooses the Modern renderer's screen-space contact-shadow method, or disables it. |
| `AntiAliasing` | `SMAA` | `Off`, `FXAA`, `SMAA` | Chooses the Modern renderer's edge-smoothing method, or disables it. |
| `Bloom` | `false` | `true`, `false` | Turns the soft glow around bright areas off or on. |
| `VolumetricLighting` | `false` | `true`, `false` | Turns depth-aware atmospheric scattering around authored lights off or on. |

`Bloom` and `VolumetricLighting` also accept `1`, `on`, `0`, and `off`.
`ToneMapper=Classic` is accepted as an older name for `Reinhard`, and
`AmbientOcclusion=GTAO` is accepted as a shorter name for `XeGTAO`. Older shared
`Brightness` and `Contrast` values are used for the selected tone mapper when
its named values are not present.

## `[OpenHP1.Touch]`

These settings configure the customizable on-screen touch overlay and virtual controls for Android and touch devices.

| Key | Default | Accepted values | What it does |
| --- | --- | --- | --- |
| `Enabled` | `true` on Android, `false` otherwise | `true`, `false` | Enables or completely disables the virtual on-screen touch controls. |
| `Opacity` | `0.75` | `0.1` to `1.0` | Controls visual transparency of all virtual buttons and stick. |
| `LookSensitivity` | `1.0` | `0.2` to `5.0` | Multiplier for camera rotation when dragging across the screen. |
| `StickX`, `StickY` | `0.150`, `0.720` | `0.02` to `0.98` | Normalized screen position of the virtual analog movement stick. |
| `StickRadius` | `64.0` | `20.0` to `120.0` | Physical radius in pixels of the movement stick base. |
| `StickVisible` | `true` | `true`, `false` | Controls visibility of the movement stick. |
| `CastX`, `CastY` | `0.880`, `0.720` | `0.02` to `0.98` | Normalized screen position of the Cast Spell button. |
| `CastSize` | `46.0` | `16.0` to `100.0` | Radius in pixels of the Cast Spell button. |
| `CastVisible` | `true` | `true`, `false` | Controls visibility of the Cast Spell button. |
| `JumpX`, `JumpY` | `0.770`, `0.820` | `0.02` to `0.98` | Normalized screen position of the Jump button. |
| `JumpSize` | `40.0` | `16.0` to `100.0` | Radius in pixels of the Jump button. |
| `JumpVisible` | `true` | `true`, `false` | Controls visibility of the Jump button. |
| `InteractX`, `InteractY` | `0.880`, `0.520` | `0.02` to `0.98` | Normalized screen position of the Use / Interact button. |
| `InteractSize` | `36.0` | `16.0` to `100.0` | Radius in pixels of the Use / Interact button. |
| `InteractVisible` | `true` | `true`, `false` | Controls visibility of the Use / Interact button. |
| `SneakX`, `SneakY` | `0.150`, `0.420` | `0.02` to `0.98` | Normalized screen position of the Walk / Sneak button. |
| `SneakSize` | `32.0` | `16.0` to `100.0` | Radius in pixels of the Walk / Sneak button. |
| `SneakVisible` | `true` | `true`, `false` | Controls visibility of the Walk / Sneak button. |
| `BroomBoostX`, `BroomBoostY` | `0.760`, `0.630` | `0.02` to `0.98` | Normalized screen position of the Broom Boost button. |
| `BroomBoostSize` | `34.0` | `16.0` to `100.0` | Radius in pixels of the Broom Boost button. |
| `BroomBoostVisible` | `true` | `true`, `false` | Controls visibility of the Broom Boost button. |
| `BroomBrakeX`, `BroomBrakeY` | `0.760`, `0.470` | `0.02` to `0.98` | Normalized screen position of the Broom Brake button. |
| `BroomBrakeSize` | `34.0` | `16.0` to `100.0` | Radius in pixels of the Broom Brake button. |
| `BroomBrakeVisible` | `true` | `true`, `false` | Controls visibility of the Broom Brake button. |
| `MenuX`, `MenuY` | `0.940`, `0.080` | `0.02` to `0.98` | Normalized screen position of the Pause Menu button. |
| `MenuSize` | `26.0` | `16.0` to `100.0` | Radius in pixels of the Pause Menu button. |
| `MenuVisible` | `true` | `true`, `false` | Controls visibility of the Pause Menu button. |
| `ConsoleX`, `ConsoleY` | `0.060`, `0.080` | `0.02` to `0.98` | Normalized screen position of the Console button. |
| `ConsoleSize` | `26.0` | `16.0` to `100.0` | Radius in pixels of the Console button. |
| `ConsoleVisible` | `true` | `true`, `false` | Controls visibility of the Console button. |

You can also customize button positions and sizes visually by tapping **Options -> Touch Controls -> Edit On-Screen Controls** in the in-game menu or launcher. In edit mode, drag any button to move it across the screen, or select it to adjust its size with the slider.

## Recovering from a bad setting

If OpenHP1 cannot understand a setting, it uses the default for that setting.
To restore every default, close the game and rename or delete `OpenHP1.ini`.
OpenHP1 will generate a fresh copy the next time it starts.

// SPDX-License-Identifier: MPL-2.0

//! Column-major 4x4 matrix math and Cover Flow card placement.
//!
//! This is the single source of truth for where each card sits in 3D: the
//! widget uses it to hit-test clicks (projecting card centres to screen x), and
//! the GPU pipeline uses the same placement to build each card's MVP matrix, so
//! what you click is exactly what you see.

/// Column-major 4x4 matrix (matches WGSL `mat4x4<f32>` memory layout). Element
/// `(row r, col c)` lives at index `c * 4 + r`.
pub type Mat4 = [f32; 16];

// --- Tunables (world units; the card half-height is the reference scale) ----
pub const HALF_H: f32 = 0.6;
pub const CARD_ASPECT: f32 = 1.5;
pub const HALF_W: f32 = HALF_H * CARD_ASPECT;

/// x of the first card on each side of the centre.
const CENTER_GAP: f32 = 1.85;
/// far cards stack asymptotically within `CENTER_GAP + EDGE_SPAN` of the centre,
/// so no matter how many windows there are they never march off-screen.
const EDGE_SPAN: f32 = 1.7;
/// how quickly the far stack packs toward the edge.
const COMPRESS: f32 = 0.4;
/// rotation of a fully side-on card (~65°).
const SIDE_ANGLE: f32 = 1.13;
/// how far each step from centre recedes into the screen.
const DEPTH: f32 = 0.55;
/// cap the recede so distant cards don't shrink to nothing — they pile into a
/// tight, fixed-size deck at each edge instead.
const DEPTH_CAP: f32 = 3.0;
/// forward pop applied to the centred card so it reads as "selected". Kept
/// modest so the popped (larger) card's top still clears the frustum.
const FOCUS_POP: f32 = 0.5;
/// gap between a card and its reflection.
const REFLECT_GAP: f32 = 0.06;
/// lift the whole scene up so the reflection falls inside the visible frustum
/// (cards sit in the upper half, reflections fill the lower half) — but not so
/// far that the cards' tops clip under the title above.
const SHIFT_Y: f32 = 0.24;

const EYE_Z: f32 = 3.3;
const FOVY: f32 = 0.70; // ~40°
const NEAR: f32 = 0.1;
const FAR: f32 = 100.0;

/// Cards beyond this offset from the centre are not drawn. High enough that
/// windows don't vanish from a large set; far cards pile into the edge deck (and
/// occlude each other) so the exact value isn't visually distinguishable.
pub const VISIBLE_SPAN: f32 = 30.0;

/// Where one card sits, derived purely from its offset `d = index - scroll`.
#[derive(Clone, Copy)]
pub struct Placement {
    pub x: f32,
    pub z: f32,
    pub rot: f32,
    /// Edge-anchor blend: 0 at the focal card (centred), +1 on the right, −1 on
    /// the left. In the model it scales `(HALF_W - half_w)` — each card is nudged
    /// outward/inward by how far its own width differs from the nominal card, so
    /// every side card's *outer* edge lands on the same line as a nominal card
    /// would, without collapsing the spread or hiding cards behind the focus.
    pub edge_anchor: f32,
}

/// Resolve a card's placement from its continuous offset to the focal point.
pub fn placement(d: f32) -> Placement {
    let ad = d.abs();
    let sgn = if d < 0.0 { -1.0 } else { 1.0 };
    let t = ad.min(1.0);
    let smooth = t * t * (3.0 - 2.0 * t); // smoothstep(0,1,t)

    // Focal band fans out to CENTER_GAP; beyond that the offset approaches
    // CENTER_GAP + EDGE_SPAN asymptotically so far cards never leave the scene.
    let beyond = (ad - 1.0).max(0.0);
    let x = sgn * (CENTER_GAP * smooth + EDGE_SPAN * (1.0 - (-beyond * COMPRESS).exp()));
    // Left cards (d<0) rotate to face right; right cards face left.
    let rot = -sgn * SIDE_ANGLE * smooth;
    let z = -ad.min(DEPTH_CAP) * DEPTH + (1.0 - t) * FOCUS_POP;
    // Align outer edges: nudge each card by its width delta from the nominal
    // card (applied in the model), keeping the focal card centred.
    let edge_anchor = sgn * smooth;
    Placement {
        x,
        z,
        rot,
        edge_anchor,
    }
}

/// Model matrix for the card face. `half_w` is per-card so each window keeps its
/// own aspect ratio (uniform height, width = `HALF_H * aspect`), edge-anchored so
/// the deck lines up along its outer edge.
pub fn card_model(p: &Placement, half_w: f32) -> Mat4 {
    let edge = p.edge_anchor * (HALF_W - half_w);
    let m = mul(translate(p.x, SHIFT_Y, p.z), rotate_y(p.rot));
    let m = mul(m, translate(edge, 0.0, 0.0));
    mul(m, scale(half_w, HALF_H, 1.0))
}

/// Model matrix for the mirrored reflection below the card (negative Y scale).
pub fn reflection_model(p: &Placement, half_w: f32) -> Mat4 {
    let edge = p.edge_anchor * (HALF_W - half_w);
    let y = SHIFT_Y - 2.0 * HALF_H - REFLECT_GAP;
    let m = mul(translate(p.x, y, p.z), rotate_y(p.rot));
    let m = mul(m, translate(edge, 0.0, 0.0));
    mul(m, scale(half_w, -HALF_H, 1.0))
}

/// Identity matrix (used for the screen-space floor/table quad).
pub fn identity_mat() -> Mat4 {
    identity()
}

/// Half-size of the square app-icon badge, in world units.
pub const BADGE_HALF: f32 = 0.17;

/// Model for the app-icon badge: bottom-centre of the (edge-anchored) card, on
/// the card plane, nudged slightly forward to avoid z-fighting with the face.
pub fn badge_model(p: &Placement, half_w: f32) -> Mat4 {
    let edge = p.edge_anchor * (HALF_W - half_w);
    let base = mul(translate(p.x, SHIFT_Y, p.z), rotate_y(p.rot));
    let local = mul(
        translate(edge, -HALF_H + BADGE_HALF, 0.02),
        scale(BADGE_HALF, BADGE_HALF, 1.0),
    );
    mul(base, local)
}

/// Combined view·projection for the given viewport aspect ratio.
pub fn view_proj(aspect: f32) -> Mat4 {
    let view = translate(0.0, 0.0, -EYE_Z);
    mul(perspective(FOVY, aspect.max(0.01), NEAR, FAR), view)
}

/// Screen-space `uv.y` (0 = top, 1 = bottom) of the focal card's bottom edge —
/// used to anchor the table's top edge so the card sits on it (no gap).
pub fn focal_base_uv_y(aspect: f32) -> f32 {
    let vp = view_proj(aspect);
    // Focal card (d = 0): x = 0, z = FOCUS_POP, bottom at y = SHIFT_Y - HALF_H.
    let v = transform(&vp, [0.0, SHIFT_Y - HALF_H, FOCUS_POP, 1.0]);
    let w = if v[3].abs() < 1e-6 { 1e-6 } else { v[3] };
    let ndc_y = v[1] / w;
    (0.5 - ndc_y * 0.5).clamp(0.0, 1.0)
}

/// Project a card centre to a screen-space x (logical pixels) for hit-testing.
pub fn project_center_x(vp: &Mat4, p: &Placement, bounds_x: f32, bounds_w: f32) -> f32 {
    let v = transform(vp, [p.x, 0.0, p.z, 1.0]);
    let w = if v[3].abs() < 1e-6 { 1e-6 } else { v[3] };
    let ndc_x = v[0] / w;
    bounds_x + (ndc_x * 0.5 + 0.5) * bounds_w
}

// --- matrix primitives ------------------------------------------------------

pub fn mul(a: Mat4, b: Mat4) -> Mat4 {
    let mut out = [0.0f32; 16];
    for c in 0..4 {
        for r in 0..4 {
            let mut s = 0.0;
            for k in 0..4 {
                s += a[k * 4 + r] * b[c * 4 + k];
            }
            out[c * 4 + r] = s;
        }
    }
    out
}

pub fn transform(m: &Mat4, v: [f32; 4]) -> [f32; 4] {
    let mut out = [0.0f32; 4];
    for r in 0..4 {
        out[r] = m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r] * v[3];
    }
    out
}

fn translate(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = identity();
    m[12] = x;
    m[13] = y;
    m[14] = z;
    m
}

fn scale(x: f32, y: f32, z: f32) -> Mat4 {
    let mut m = [0.0f32; 16];
    m[0] = x;
    m[5] = y;
    m[10] = z;
    m[15] = 1.0;
    m
}

fn rotate_y(rad: f32) -> Mat4 {
    let (s, c) = rad.sin_cos();
    let mut m = identity();
    m[0] = c;
    m[2] = -s;
    m[8] = s;
    m[10] = c;
    m
}

/// Right-handed perspective mapping z into [0, 1] (wgpu clip space).
fn perspective(fovy: f32, aspect: f32, near: f32, far: f32) -> Mat4 {
    let f = 1.0 / (fovy * 0.5).tan();
    let mut m = [0.0f32; 16];
    m[0] = f / aspect;
    m[5] = f;
    m[10] = far / (near - far);
    m[11] = -1.0;
    m[14] = (far * near) / (near - far);
    m
}

fn identity() -> Mat4 {
    let mut m = [0.0f32; 16];
    m[0] = 1.0;
    m[5] = 1.0;
    m[10] = 1.0;
    m[15] = 1.0;
    m
}

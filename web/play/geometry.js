/**
 * The playground and the land it sits in.
 *
 * Everything is built from primitives, so the scene has no asset dependencies
 * and every part can be restyled at runtime. Meshes carry a `role` in userData
 * instead of a material; the active style decides what a role looks like. That
 * is what lets the same geometry read as a toy, a blueprint or an ASCII dump
 * without rebuilding anything.
 *
 * Two settings are available. `island` is a plate in an ocean; `meadow` is open
 * ground running past the frame with a stream through it and woods at the edge.
 * They share every piece of play equipment and differ only in the land, so the
 * switch below is the whole difference between them.
 */
import * as THREE from 'three';

export const WORLD = 'island';   // 'island' | 'meadow'

const T = Math.PI * 2;

// --- terrain ---------------------------------------------------------------

function h2(x, y) {
  let n = Math.imul(x | 0, 374761393) ^ Math.imul(y | 0, 668265263);
  n = Math.imul(n ^ (n >>> 13), 1274126177);
  return ((n ^ (n >>> 16)) >>> 0) / 4294967296;
}

function vnoise(x, y) {
  const xi = Math.floor(x), yi = Math.floor(y);
  const xf = x - xi, yf = y - yi;
  const u = xf * xf * (3 - 2 * xf), v = yf * yf * (3 - 2 * yf);
  const a = h2(xi, yi), b = h2(xi + 1, yi), c = h2(xi, yi + 1), d = h2(xi + 1, yi + 1);
  return a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v;
}

const fbm = (x, y) => vnoise(x, y) * 0.55 + vnoise(x * 2.1 + 9, y * 2.1 - 4) * 0.3
  + vnoise(x * 4.3 - 7, y * 4.3 + 2) * 0.15;

function smoothstep(e0, e1, x) {
  const t = Math.max(0, Math.min(1, (x - e0) / (e1 - e0)));
  return t * t * (3 - 2 * t);
}

/** Where the stream runs, as a function of depth into the scene. */
export const streamX = (z) => -10 + Math.sin(z * 0.075) * 3.0 + Math.sin(z * 0.028 + 1.3) * 2.0;

// A brook, not a gorge. The first cut was 2.9 deep with the surface well below
// the meadow, and at the shallow angle the camera actually uses, the near bank
// hid the water completely: all you saw was a dark seam in the grass.
const WATER_Y = -0.72;
const CHANNEL_DEPTH = 1.45;

/**
 * Ground height. Flat under the playground, rolling further out, with a valley
 * cut along the stream. The flat pad matters: displacing the ground under the
 * swing frame would leave its legs hanging.
 */
export function groundHeight(x, z) {
  if (WORLD === 'island') return 0;
  const r = Math.hypot(x, z);
  const roll = (fbm(x * 0.042, z * 0.042) - 0.5) * 6.5;
  let y = roll * smoothstep(7.5, 30, r);
  const d = Math.abs(x - streamX(z));
  y -= CHANNEL_DEPTH * (1 - smoothstep(1.6, 5.0, d));
  return y;
}

// --- helpers ---------------------------------------------------------------

function put(parent, mesh, role, opts = {}) {
  mesh.userData.role = role;
  if (opts.pos) mesh.position.set(...opts.pos);
  if (opts.rot) mesh.rotation.set(...opts.rot);
  if (opts.scale) mesh.scale.set(...opts.scale);
  // Recorded as intent, not just as state. applyMaterials rewrites the live
  // flags on every style change, so without this the opt-outs below are lost
  // the moment a treatment is applied.
  mesh.userData.cast = opts.shadow !== false;
  mesh.userData.receive = opts.receive !== false;
  mesh.castShadow = mesh.userData.cast;
  mesh.receiveShadow = mesh.userData.receive;
  parent.add(mesh);
  return mesh;
}

const box = (w, h, d) => new THREE.BoxGeometry(w, h, d);
const cyl = (rt, rb, h, s = 12) => new THREE.CylinderGeometry(rt, rb, h, s);
const sph = (r, s = 14) => new THREE.SphereGeometry(r, s, s * 0.7);
const cone = (r, h, s = 4) => new THREE.ConeGeometry(r, h, s);

/** Deterministic scatter, so the landscape is the same on every load. */
function rng(seed) {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s ^ (s >>> 15), 2246822519) + 374761393) >>> 0;
    s = (s ^ (s >>> 13)) >>> 0;
    return (Math.imul(s, 3266489917) >>> 0) / 4294967296;
  };
}

function ladder(group, h, rungs, role) {
  for (const s of [-0.22, 0.22]) {
    put(group, new THREE.Mesh(cyl(0.05, 0.05, h, 6), null), role, { pos: [s, h / 2, 0] });
  }
  for (let i = 1; i < rungs; i++) {
    put(group, new THREE.Mesh(cyl(0.04, 0.04, 0.5, 6), null), role,
      { pos: [0, (i / rungs) * h, 0], rot: [0, 0, Math.PI / 2] });
  }
}

/**
 * Curved slide, built from a chain of short slabs following a quadratic drop.
 * A tube would be smoother and would read as a pipe; a run of flat segments
 * keeps the silhouette of an actual slide.
 */
function slide(group, from, to, role) {
  const n = 14;
  const [x0, y0, z0] = from, [x1, y1, z1] = to;
  let prev = new THREE.Vector3(x0, y0, z0);
  for (let i = 1; i <= n; i++) {
    const t = i / n;
    const p = new THREE.Vector3(
      x0 + (x1 - x0) * t,
      y0 + (y1 - y0) * (t * t * (3 - 2 * t)),
      z0 + (z1 - z0) * t,
    );
    const mid = prev.clone().lerp(p, 0.5);
    const len = prev.distanceTo(p);
    const seg = new THREE.Mesh(box(0.66, 0.07, len * 1.25), null);
    seg.position.copy(mid);
    seg.lookAt(p);
    put(group, seg, role, { shadow: true });
    for (const s of [-0.32, 0.32]) {
      const rail = new THREE.Mesh(box(0.06, 0.14, len * 1.25), null);
      rail.position.copy(mid);
      rail.lookAt(p);
      rail.translateX(s);
      rail.translateY(0.09);
      put(group, rail, role, { shadow: false });
    }
    prev = p;
  }
}

/**
 * Bake a colour per vertex so the sea is not one flat sheet.
 *
 * These are multipliers over whatever colour the active style picked for the
 * water role, so the treatment still decides the hue and this only decides how
 * it varies. Two things drive it: a sum of sines at three scales, which breaks
 * the surface into patches the way open water actually reads, and a shore term
 * that lightens the ring around the island, which is the shallows.
 *
 * No noise library and no randomness. Sines are cheap, tile without a seam, and
 * are identical on every load, which matters because everything else in this
 * scene is deterministic too.
 */
/**
 * How much swell each vertex is allowed, by distance from the island.
 *
 * The far water has to be flat. Seen from the camera the surface near the
 * horizon is almost edge on, so a great many geometry rows fall inside one
 * buffer pixel, and the swell's crests and troughs shade light and dark
 * through the vertex normals. Compressed like that they stop reading as waves
 * and alias into horizontal bands across the top of the frame.
 *
 * Precomputed once because the alternative is a square root per vertex per
 * frame, and there are five thousand of them.
 */
function swellFade(geo) {
  const pos = geo.attributes.position;
  const out = new Float32Array(pos.count);
  for (let i = 0; i < pos.count; i++) {
    const x = pos.array[i * 3], y = pos.array[i * 3 + 1];
    const r = Math.sqrt(x * x + y * y);
    const f = 1 - Math.min(1, Math.max(0, (r - 16) / 22));
    out[i] = f * f;   // squared, so it is already gentle well before it is gone
  }
  return out;
}

function paintOcean(geo) {
  const pos = geo.attributes.position;
  const col = new Float32Array(pos.count * 3);
  // Wide, because the pixel pass posterises to six levels per channel and a
  // narrow spread lands almost entirely in one bucket. Measured on the first
  // attempt: 74% of the sea came out a single tone. These reach across three
  // or four buckets instead.
  const DEEP = [0.46, 0.60, 0.84];
  const SHALLOW = [1.12, 1.30, 1.28];

  for (let i = 0; i < pos.count; i++) {
    const x = pos.array[i * 3], y = pos.array[i * 3 + 1];

    // All three terms are deliberately short. The first pass used wavelengths
    // of 90 to 130 units against a visible sea only about 120 across, so a
    // single trough filled a quarter of the frame and read as a dark shape
    // floating in the water rather than as surface. At 8 to 15 units it is
    // texture, which is what a sea at this distance actually looks like.
    const n = Math.sin(x * 0.42 + y * 0.19)
      + 0.62 * Math.cos(x * 0.23 - y * 0.51)
      + 0.36 * Math.sin(x * 0.77 + y * 0.63);

    // Lighter within about 20 units of the island, falling off smoothly.
    const r = Math.sqrt(x * x + y * y);
    const shore = 1 - Math.min(1, Math.max(0, (r - 6.5) / 14));

    // Detail fades out past about 22 units and is gone by 48. Near the horizon
    // the surface is seen almost edge on, so dozens of geometry rows land
    // inside a single buffer pixel, and at a render scale of 0.2 that buffer is
    // tiny. Any variation left out there stops being texture and aliases into
    // horizontal bands instead. Flattening the far field is the fix; nothing is
    // lost, because at that angle there was never enough room to show it.
    const detail = 1 - Math.min(1, Math.max(0, (r - 22) / 26));

    // A sum of sines is bell shaped and would pile up in the middle, which is
    // the same problem again. The wide coefficient drives it past 0 and 1 often
    // enough that the clamp does the work, giving patches rather than a haze.
    let m = 0.5 + n * 0.34 * detail;
    m = Math.min(1, Math.max(0, m * 0.82 + shore * 0.5));

    for (let k = 0; k < 3; k++) col[i * 3 + k] = DEEP[k] + (SHALLOW[k] - DEEP[k]) * m;
  }
  geo.setAttribute('color', new THREE.BufferAttribute(col, 3));
}

function pine(group, x, z, scale, rot) {
  const g = new THREE.Group();
  g.position.set(x, groundHeight(x, z), z);
  g.scale.setScalar(scale);
  g.rotation.y = rot;
  put(g, new THREE.Mesh(cyl(0.13, 0.2, 1.2, 6), null), 'trunk', { pos: [0, 0.6, 0] });
  put(g, new THREE.Mesh(cone(0.8, 1.3, 7), null), 'foliage', { pos: [0, 1.65, 0] });
  put(g, new THREE.Mesh(cone(0.62, 1.05, 7), null), 'foliage', { pos: [0, 2.3, 0] });
  group.add(g);
  return g;
}

/** Broadleaf: a trunk under a few overlapping canopy blobs. */
function broadleaf(group, x, z, scale, rot) {
  const g = new THREE.Group();
  g.position.set(x, groundHeight(x, z), z);
  g.scale.setScalar(scale);
  g.rotation.y = rot;
  put(g, new THREE.Mesh(cyl(0.16, 0.28, 1.7, 7), null), 'trunk', { pos: [0, 0.85, 0] });
  put(g, new THREE.Mesh(new THREE.IcosahedronGeometry(1.05, 0), null), 'foliage',
    { pos: [0, 2.35, 0], rot: [0.3, rot * 2, 0.2] });
  put(g, new THREE.Mesh(new THREE.IcosahedronGeometry(0.72, 0), null), 'foliage',
    { pos: [0.55, 1.95, 0.3], rot: [0.6, rot, 0.4] });
  put(g, new THREE.Mesh(new THREE.IcosahedronGeometry(0.66, 0), null), 'foliage',
    { pos: [-0.5, 2.05, -0.35], rot: [0.2, rot * 3, 0.7] });
  group.add(g);
  return g;
}


/** Open ground with a stream and woods, used when WORLD is 'meadow'. */
function buildMeadow(root, anim) {
  const SIZE = 170, SEG = 104;
  const groundGeo = new THREE.PlaneGeometry(SIZE, SIZE, SEG, SEG);
  {
    const pos = groundGeo.attributes.position;
    // The plane is built in XY and then laid flat, so its local y maps to world
    // -z and its local z becomes world height.
    for (let i = 0; i < pos.count; i++) {
      pos.setZ(i, groundHeight(pos.getX(i), -pos.getY(i)));
    }
    pos.needsUpdate = true;
    groundGeo.computeVertexNormals();
  }
  put(root, new THREE.Mesh(groundGeo, null), 'grass', { rot: [-Math.PI / 2, 0, 0], shadow: false });

  {
    const half = 3.3, from = -80, to = 80, step = 2;
    const n = Math.floor((to - from) / step) + 1;
    const verts = new Float32Array(n * 2 * 3);
    const idx = [];
    for (let i = 0; i < n; i++) {
      const z = from + i * step;
      const cx = streamX(z);
      const tang = (streamX(z + 1) - streamX(z - 1)) * 0.5;
      const len = Math.hypot(tang, 1);
      const nx = 1 / len, nz = -tang / len;
      verts.set([cx - nx * half, WATER_Y, z - nz * half], i * 6);
      verts.set([cx + nx * half, WATER_Y, z + nz * half], i * 6 + 3);
      if (i < n - 1) {
        // Wind counter-clockwise seen from above. The obvious ordering faces
        // the ribbon downward, and a front-side material then culls it.
        const a = i * 2;
        idx.push(a, a + 2, a + 1, a + 1, a + 2, a + 3);
      }
    }
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(verts, 3));
    g.setIndex(idx);
    g.computeVertexNormals();
    anim.water = put(root, new THREE.Mesh(g, null), 'water', { shadow: false, receive: false });
    anim.waterKind = 'ribbon';
    anim.waterBase = Float32Array.from(verts);
  }

  {
    const r = rng(4711);
    for (let i = 0; i < 26; i++) {
      const z = -34 + r() * 68;
      const x = streamX(z) + (r() - 0.5) * 7.5;
      const s = 0.18 + r() * 0.42;
      const y = Math.max(groundHeight(x, z), WATER_Y - 0.3) + s * 0.4;
      put(root, new THREE.Mesh(new THREE.DodecahedronGeometry(s, 0), null), 'rock',
        { pos: [x, y, z], rot: [r() * 3, r() * 3, r() * 3] });
    }
  }

  // Rejection sampling: clear of the play area, out of the water, thickening
  // with distance so the far edge reads as a treeline.
  {
    const r = rng(90210);
    let placed = 0, tries = 0;
    while (placed < 58 && tries < 5000) {
      tries++;
      const a = r() * T;
      const rad = 7 + Math.pow(r(), 0.62) * 40;
      const x = Math.cos(a) * rad, z = Math.sin(a) * rad;
      if (Math.hypot(x, z) < 7.2) continue;
      if (Math.abs(x - streamX(z)) < 5.2) continue;
      const s = 0.75 + r() * 0.75;
      if (r() < 0.45) pine(root, x, z, s * 1.1, r() * T);
      else broadleaf(root, x, z, s, r() * T);
      placed++;
    }
    broadleaf(root, -6.4, -4.9, 1.9, 0.7);
    pine(root, 7.4, -3.2, 1.7, 2.1);
  }

  // Instanced: too many to justify a mesh each, and small enough that the
  // styles which outline geometry can skip them.
  {
    const r = rng(1337);
    const n = 220;
    const m = new THREE.InstancedMesh(sph(0.09, 6), null, n);
    m.userData.role = 'flower';
    m.castShadow = false;
    m.receiveShadow = false;
    const mat = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const v = new THREE.Vector3(), sc = new THREE.Vector3(1, 1, 1);
    for (let i = 0; i < n; i++) {
      const a = r() * T;
      const rad = 5.5 + Math.pow(r(), 0.7) * 26;
      const x = Math.cos(a) * rad, z = Math.sin(a) * rad;
      const inWater = Math.abs(x - streamX(z)) < 3.6;
      v.set(x, inWater ? -50 : groundHeight(x, z) + 0.1, z);
      sc.setScalar(0.7 + r() * 0.8);
      mat.compose(v, q, sc);
      m.setMatrixAt(i, mat);
    }
    root.add(m);
  }
}

// --- scene -----------------------------------------------------------------

export function buildPlayground() {
  const root = new THREE.Group();
  const anim = {};
  // The island carries a grass disc; the meadow does not, so the equipment
  // sits a little lower there.
  const B = WORLD === 'island' ? 0.14 : 0.02;

  if (WORLD === 'island') {
    const island = new THREE.Group();
    root.add(island);
    put(island, new THREE.Mesh(cyl(6.2, 5.5, 1.0, 8), null), 'ground', { pos: [0, -0.5, 0] });
    put(island, new THREE.Mesh(cyl(6.24, 6.24, 0.14, 8), null), 'grass', { pos: [0, 0.02, 0] });

    // 240 across rather than 90. The camera sits about 6.7 above the surface
    // and its top ray leaves at roughly 7 degrees below horizontal, so it meets
    // the water some 63 units out; at the old half-width of 45 the sheet simply
    // ended inside the frame and the sky showed through along the top edge as a
    // pale band. 120 clears that with room for the orbit to swing.
    const sea = new THREE.PlaneGeometry(240, 240, 72, 72);
    paintOcean(sea);
    // Neither casts nor receives. The shadow camera is an 11 unit box reaching
    // 46 deep, sized for the island; the sea is 120 out. Everything past that
    // box samples off the edge of the depth map, which is what put the dark
    // horizontal bands across the far water.
    const water = put(root, new THREE.Mesh(sea, null), 'water',
      { pos: [0, -0.72, 0], rot: [-Math.PI / 2, 0, 0], shadow: false, receive: false });
    anim.water = water;
    anim.waterKind = 'plane';
    anim.waterBase = Float32Array.from(water.geometry.attributes.position.array);
    anim.waterFade = swellFade(sea);

    pine(root, -4.3, -2.6, 1.15, 0.4);
    pine(root, 4.35, -2.0, 0.92, 2.2);
    pine(root, -4.6, 2.4, 0.8, 1.1);
    pine(root, 3.9, 3.5, 1.0, 3.0);

    for (const [x, z, r] of [[3.1, -2.4, 0.5], [-3.5, -1.2, 0.42], [2.7, 3.3, 0.36]]) {
      put(root, new THREE.Mesh(new THREE.DodecahedronGeometry(r, 0), null), 'rock',
        { pos: [x, 0.14 + r * 0.4, z], rot: [0.3, r * 9, 0.2] });
    }
  } else {
    buildMeadow(root, anim);
  }

  // --- sand pit -----------------------------------------------------------
  put(root, new THREE.Mesh(cyl(3.7, 3.7, 0.16, 6), null), 'sand',
    { pos: [0.1, B - 0.05, 0.2], rot: [0, 0.3, 0], shadow: false });

  // --- swing set ----------------------------------------------------------
  const swingSet = new THREE.Group();
  swingSet.position.set(-2.25, B, 1.15);
  swingSet.rotation.y = 0.35;
  root.add(swingSet);
  for (const s of [-1.05, 1.05]) {
    for (const t of [-0.5, 0.5]) {
      put(swingSet, new THREE.Mesh(cyl(0.06, 0.07, 2.1, 8), null), 'metalB',
        { pos: [s, 1.0, t], rot: [t * 0.22, 0, -s * 0.14] });
    }
  }
  put(swingSet, new THREE.Mesh(cyl(0.07, 0.07, 2.4, 8), null), 'metalB',
    { pos: [0, 1.98, 0], rot: [0, 0, Math.PI / 2] });
  anim.swings = [];
  for (const s of [-0.45, 0.45]) {
    const pivot = new THREE.Group();
    pivot.position.set(s, 1.98, 0);
    swingSet.add(pivot);
    for (const r of [-0.22, 0.22]) {
      put(pivot, new THREE.Mesh(cyl(0.018, 0.018, 1.25, 5), null), 'rope',
        { pos: [r, -0.62, 0], shadow: false });
    }
    put(pivot, new THREE.Mesh(box(0.56, 0.07, 0.28), null), 'plastic', { pos: [0, -1.25, 0] });
    anim.swings.push(pivot);
  }

  // --- tower, roof and slide ----------------------------------------------
  const tower = new THREE.Group();
  tower.position.set(1.65, B, -0.55);
  tower.rotation.y = -0.4;
  root.add(tower);
  for (const [sx, sz] of [[-0.62, -0.62], [0.62, -0.62], [-0.62, 0.62], [0.62, 0.62]]) {
    put(tower, new THREE.Mesh(box(0.16, 1.75, 0.16), null), 'wood', { pos: [sx, 0.87, sz] });
  }
  put(tower, new THREE.Mesh(box(1.6, 0.14, 1.6), null), 'wood', { pos: [0, 1.8, 0] });
  for (const [sx, sz, rw] of [[0, -0.75, 0], [-0.75, 0, 1], [0.75, 0, 1]]) {
    put(tower, new THREE.Mesh(box(rw ? 0.1 : 1.6, 0.5, rw ? 1.6 : 0.1), null), 'wood',
      { pos: [sx, 2.12, sz] });
  }
  for (const [sx, sz] of [[-0.62, -0.62], [0.62, -0.62], [-0.62, 0.62], [0.62, 0.62]]) {
    put(tower, new THREE.Mesh(box(0.13, 0.95, 0.13), null), 'wood', { pos: [sx, 2.4, sz] });
  }
  put(tower, new THREE.Mesh(cone(1.55, 1.0, 4), null), 'roof',
    { pos: [0, 3.35, 0], rot: [0, Math.PI / 4, 0] });
  const lad = new THREE.Group();
  lad.position.set(-0.62, 0, 0.9);
  lad.rotation.x = -0.18;
  tower.add(lad);
  ladder(lad, 1.85, 6, 'wood');
  slide(tower, [0.66, 1.72, 0.3], [2.5, 0.06, 1.5], 'slideDeck');

  // --- see-saw ------------------------------------------------------------
  const seesaw = new THREE.Group();
  seesaw.position.set(-1.5, B + 0.02, -1.85);
  seesaw.rotation.y = 0.9;
  root.add(seesaw);
  put(seesaw, new THREE.Mesh(cyl(0.1, 0.24, 0.42, 7), null), 'metalA', { pos: [0, 0.21, 0] });
  const plank = new THREE.Group();
  plank.position.y = 0.44;
  seesaw.add(plank);
  put(plank, new THREE.Mesh(box(2.7, 0.1, 0.3), null), 'plastic');
  for (const s of [-1.15, 1.15]) {
    put(plank, new THREE.Mesh(cyl(0.035, 0.035, 0.3, 6), null), 'metalA', { pos: [s, 0.18, 0] });
  }
  anim.seesaw = plank;

  // --- roundabout ---------------------------------------------------------
  const round = new THREE.Group();
  round.position.set(2.15, B + 0.02, 2.35);
  root.add(round);
  put(round, new THREE.Mesh(cyl(0.12, 0.16, 0.3, 8), null), 'metalA', { pos: [0, 0.15, 0] });
  const disc = new THREE.Group();
  disc.position.y = 0.32;
  round.add(disc);
  put(disc, new THREE.Mesh(cyl(1.0, 0.95, 0.12, 14), null), 'plasticB');
  for (let i = 0; i < 4; i++) {
    const a = (i / 4) * T;
    put(disc, new THREE.Mesh(cyl(0.035, 0.035, 0.42, 6), null), 'metalA',
      { pos: [Math.cos(a) * 0.66, 0.27, Math.sin(a) * 0.66] });
  }
  anim.roundabout = disc;

  // --- sandbox props ------------------------------------------------------
  put(root, new THREE.Mesh(cyl(0.17, 0.2, 0.24, 9), null), 'plasticC', { pos: [-0.55, B + 0.12, 1.75] });
  put(root, new THREE.Mesh(sph(0.22), null), 'plasticA', { pos: [0.75, B + 0.22, 2.05] });
  put(root, new THREE.Mesh(box(0.38, 0.28, 0.38), null), 'plasticB',
    { pos: [-1.3, B + 0.14, 2.3], rot: [0, 0.5, 0] });

  // --- utility pad --------------------------------------------------------
  // The one corner of the playground that is not for playing.
  //
  // Two things came out of the render being 240 pixels across, which puts
  // roughly twelve pixels on a world unit. The cabinets are pale rather than
  // slate, because dark grey against grass collapsed into a silhouette with no
  // readable detail. And they sit a clear gap apart, because at the first
  // spacing they touched and merged into a single mass.
  //
  // Everything uses roles the styles already define, so no material has to be
  // added to the other seven treatments.
  const pad = new THREE.Group();
  // Set in from the rim. At the first placement the pad's far corners reached
  // 5.79 from the centre against an island inradius of 5.77, so it sat exactly
  // on the edge and read as though it were about to slide off. The slab is
  // also narrower now that the mast is gone and only the two cabinets need
  // room; that alone pulls the corners in by a third of a unit.
  pad.position.set(-0.8, B, 4.25);
  pad.rotation.y = -0.2;
  root.add(pad);
  put(pad, new THREE.Mesh(box(2.3, 0.1, 1.0), null), 'rock', { pos: [0, 0.05, 0], shadow: false });

  for (const dx of [-0.72, 0.72]) {
    put(pad, new THREE.Mesh(box(0.72, 1.15, 0.56), null), 'rock', { pos: [dx, 0.68, 0] });
    put(pad, new THREE.Mesh(box(0.76, 0.09, 0.6), null), 'metalA', { pos: [dx, 1.29, 0] });
    put(pad, new THREE.Mesh(box(0.58, 0.1, 0.04), null), 'metalA', { pos: [dx, 0.2, 0.29] });

    // Rack units, as shallow dark grooves rather than lit indicators. The
    // first version put saturated bars across the front and blinked them, and
    // in a scene where nothing else asks for attention they were the loudest
    // thing on the island. Unlit, these give the cabinet a face without
    // competing with the playhouse for the eye.
    for (let k = 0; k < 3; k++) {
      put(pad, new THREE.Mesh(box(0.5, 0.06, 0.03), null), 'metalA',
        { pos: [dx, 1.0 - k * 0.26, 0.29], shadow: false });
    }
  }

  // --- lamps --------------------------------------------------------------
  for (const [x, z] of [[-4.6, 0.4], [4.7, 1.4]]) {
    const l = new THREE.Group();
    l.position.set(x, groundHeight(x, z), z);
    root.add(l);
    put(l, new THREE.Mesh(cyl(0.07, 0.11, 1.75, 8), null), 'metalA', { pos: [0, 0.88, 0] });
    put(l, new THREE.Mesh(cyl(0.2, 0.14, 0.16, 8), null), 'metalA', { pos: [0, 1.82, 0] });
    put(l, new THREE.Mesh(sph(0.19, 10), null), 'lamp', { pos: [0, 1.98, 0], shadow: false });
  }

  root.userData.anim = anim;
  return root;
}

/** Advance the moving parts. */
export function animatePlayground(root, t) {
  const a = root.userData.anim;
  if (!a) return;
  a.swings[0].rotation.x = Math.sin(t * 1.35) * 0.5;
  a.swings[1].rotation.x = Math.sin(t * 1.05 + 1.9) * 0.32;
  a.seesaw.rotation.z = Math.sin(t * 0.85) * 0.19;
  a.roundabout.rotation.y = t * 0.55;

  const pos = a.water.geometry.attributes.position;
  const base = a.waterBase;
  for (let i = 0; i < pos.count; i++) {
    if (a.waterKind === 'plane') {
      // Built in XY and laid flat, so the swell goes on the local z.
      const x = base[i * 3], y = base[i * 3 + 1];
      const f = a.waterFade ? a.waterFade[i] : 1;
      pos.array[i * 3 + 2] = f * (Math.sin(x * 0.28 + t * 0.9) * 0.11
        + Math.cos(y * 0.23 - t * 0.7) * 0.09);
    } else {
      const x = base[i * 3], z = base[i * 3 + 2];
      pos.array[i * 3 + 1] = base[i * 3 + 1]
        + Math.sin(z * 0.75 + t * 2.1) * 0.045
        + Math.cos(x * 0.9 - t * 1.6) * 0.03;
    }
  }
  pos.needsUpdate = true;
  a.water.geometry.computeVertexNormals();
}

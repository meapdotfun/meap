/**
 * The playground.
 *
 * One scene graph, rendered through the pixel treatment: the scene is drawn to
 * a 0.2 scale buffer and resampled onto a posterised, ordered-dithered palette.
 *
 * styles.js still defines the other treatments and setStyle still takes an id,
 * because the machinery to swap materials, lights and background is what made
 * choosing pixel possible. Nothing exposes the choice any more; the page has no
 * controls at all, and the camera turns on its own.
 */
import * as THREE from 'three';
import { buildPlayground, animatePlayground } from './geometry.js';
import { byId } from './styles.js';
import { PASSES } from './post.js';

const $ = (s) => document.querySelector(s);
const gl = $('#gl');
const fx = $('#fx');
const fxCtx = fx.getContext('2d');

const renderer = new THREE.WebGLRenderer({ canvas: gl, antialias: true, alpha: false });
renderer.setPixelRatio(Math.min(devicePixelRatio, 2));
renderer.shadowMap.type = THREE.PCFSoftShadowMap;

const scene = new THREE.Scene();
const world = buildPlayground();
scene.add(world);

let camera = new THREE.PerspectiveCamera(38, 1, 0.1, 300);
let style = null;
let bubbles = null;
let styleMaterials = [];
let edgeLines = [];
const target = new THREE.Vector3(0, 0.9, 0);

// ---------------------------------------------------------------------------

function disposeStyle() {
  for (const e of edgeLines) {
    e.parent?.remove(e);
    e.geometry.dispose();
  }
  edgeLines = [];
  for (const m of styleMaterials) m.dispose();
  styleMaterials = [];
  if (bubbles) {
    scene.remove(bubbles);
    bubbles.geometry.dispose(); bubbles.material.dispose();
    bubbles = null;
  }
  scene.environment = null;
  scene.fog = null;
  // Everything in the scene except the world was put there by a style, so
  // remove by exclusion. Testing for known types missed GridHelper, which does
  // not set an isGridHelper flag, and the blueprint grid leaked into every
  // style selected after it.
  for (const c of [...scene.children]) {
    if (c === world) continue;
    scene.remove(c);
    if (c.geometry) c.geometry.dispose();
    if (c.material) (Array.isArray(c.material) ? c.material : [c.material]).forEach((m) => m.dispose());
  }
}

function applyMaterials(s) {
  const cache = new Map();
  world.traverse((o) => {
    if (!o.isMesh) return;
    const role = o.userData.role;
    if (!cache.has(role)) cache.set(role, s.material(role));
    const mat = cache.get(role);
    // Geometry carrying baked vertex colours modulates whatever the style
    // chose. Switching it on here rather than in each style definition means
    // the ocean gets its mottling in every treatment without eight edits, and
    // a style still owns the base hue.
    if (o.geometry?.attributes.color && !mat.vertexColors) {
      mat.vertexColors = true;
      mat.needsUpdate = true;
    }
    o.material = mat;
    o.visible = true;
    // A style decides whether shadows exist at all; the geometry decides which
    // meshes take part. Assigning the style's answer unconditionally, as this
    // did, silently undid every opt-out set at build time.
    o.castShadow = !!s.shadows && o.userData.cast !== false;
    o.receiveShadow = !!s.shadows && o.userData.receive !== false;
  });
  styleMaterials.push(...cache.values());
}

/**
 * Outline every mesh. The lines are children of the mesh they came from, so
 * they inherit its transform; adding them to a shared group instead drops the
 * transform and collapses the whole scene onto the origin.
 */
function buildEdges(s) {
  const mat = new THREE.LineBasicMaterial({
    color: new THREE.Color(s.edges.color), transparent: true, opacity: s.edges.opacity,
  });
  styleMaterials.push(mat);
  world.traverse((o) => {
    if (!o.isMesh || !o.geometry) return;
    // Skip instanced meshes. A child of an InstancedMesh is drawn once at the
    // parent transform, so outlining the wildflowers would put a single stray
    // wireframe sphere at the origin rather than one per flower.
    if (o.isInstancedMesh) return;
    const e = new THREE.LineSegments(new THREE.EdgesGeometry(o.geometry, 24), mat);
    o.add(e);
    edgeLines.push(e);
  });
}

function makeBubbles() {
  const n = 46;
  const geo = new THREE.SphereGeometry(1, 10, 8);
  const mat = new THREE.MeshPhysicalMaterial({
    color: 0xffffff, roughness: 0, metalness: 0, transmission: 1, thickness: 0.4,
    transparent: true, opacity: 0.5, envMapIntensity: 2,
  });
  const m = new THREE.InstancedMesh(geo, mat, n);
  m.frustumCulled = false;
  const data = [];
  for (let i = 0; i < n; i++) {
    data.push({
      x: (Math.random() - 0.5) * 16, z: (Math.random() - 0.5) * 16,
      y: Math.random() * 9, r: 0.07 + Math.random() * 0.17,
      speed: 0.25 + Math.random() * 0.5, phase: Math.random() * 9,
    });
  }
  m.userData.data = data;
  return m;
}

function setStyle(id) {
  const s = byId(id);
  style = s;
  disposeStyle();

  renderer.shadowMap.enabled = !!s.shadows;

  const aspect = gl.clientWidth / Math.max(1, gl.clientHeight);
  if (s.camera.ortho) {
    const h = s.camera.dist * 0.42;
    camera = new THREE.OrthographicCamera(-h * aspect, h * aspect, h, -h, 0.1, 300);
  } else {
    camera = new THREE.PerspectiveCamera(s.camera.fov, aspect, 0.1, 300);
  }

  s.setup(scene, renderer);
  applyMaterials(s);
  if (s.edges) buildEdges(s);
  if (s.bubbles) { bubbles = makeBubbles(); scene.add(bubbles); }

  document.body.dataset.ui = s.ui || 'dark';
  resize();
}

// ---------------------------------------------------------------------------

function resize() {
  const w = fx.clientWidth || 1, h = fx.clientHeight || 1;
  const dpr = Math.min(devicePixelRatio, 2);
  const scale = style?.renderScale || 1;

  if (style?.grid) {
    // A character grid, not a pixel buffer: cells are about twice as tall as
    // they are wide, so the row count has to be derived from that ratio rather
    // than from the viewport aspect alone.
    const cols = style.grid.cols;
    const rows = Math.max(2, Math.round(cols * (h / w) * 0.5));
    renderer.setPixelRatio(1);
    renderer.setSize(cols, rows, false);
  } else if (style?.post && scale < 1) {
    // Reduced buffer: the pass resamples it into something else anyway.
    renderer.setPixelRatio(1);
    renderer.setSize(Math.max(2, Math.round(w * scale)), Math.max(2, Math.round(h * scale)), false);
  } else {
    renderer.setPixelRatio(dpr);
    renderer.setSize(w, h, false);
  }

  fx.width = Math.round(w * dpr);
  fx.height = Math.round(h * dpr);
  fxCtx.setTransform(dpr, 0, 0, dpr, 0, 0);

  const usePost = !!style?.post;
  gl.style.opacity = usePost ? '0' : '1';
  fx.style.opacity = usePost ? '1' : '0';

  const aspect = w / h;
  if (camera.isOrthographicCamera) {
    const hh = style.camera.dist * 0.42;
    camera.left = -hh * aspect; camera.right = hh * aspect;
    camera.top = hh; camera.bottom = -hh;
  } else {
    camera.aspect = aspect;
  }
  camera.updateProjectionMatrix();
}
addEventListener('resize', resize);

let t = 0, last = performance.now();
let yaw = 0;

function frame(now) {
  requestAnimationFrame(frame);
  const dt = Math.min(0.08, (now - last) / 1000);
  last = now;
  t += dt;

  animatePlayground(world, t);

  if (bubbles) {
    const m = new THREE.Matrix4();
    const q = new THREE.Quaternion();
    const v = new THREE.Vector3(), sc = new THREE.Vector3();
    bubbles.userData.data.forEach((b, i) => {
      b.y += b.speed * dt;
      if (b.y > 9) b.y = -0.6;
      v.set(b.x + Math.sin(t * 0.6 + b.phase) * 0.5, b.y, b.z + Math.cos(t * 0.5 + b.phase) * 0.5);
      sc.setScalar(b.r);
      m.compose(v, q, sc);
      bubbles.setMatrixAt(i, m);
    });
    bubbles.instanceMatrix.needsUpdate = true;
  }

  yaw = (style?.camera.spin || 0) * t;
  const dist = style?.camera.dist || 15;
  const cp = Math.cos(style?.camera.pitch ?? 0.5), sp = Math.sin(style?.camera.pitch ?? 0.5);
  camera.position.set(
    target.x + dist * cp * Math.cos(yaw),
    target.y + dist * sp,
    target.z + dist * cp * Math.sin(yaw),
  );
  camera.lookAt(target);

  renderer.render(scene, camera);

  if (style?.post) {
    const w = fx.clientWidth, h = fx.clientHeight;
    const pass = PASSES[style.post];
    if (pass) pass(gl, fxCtx, w, h, style.postOpts || {});
  }
}

// ---------------------------------------------------------------------------

setStyle(byId('pixel').id);
requestAnimationFrame(frame);

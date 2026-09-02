/**
 * Styles.
 *
 * A style owns four things: the palette it assigns to each role, the lighting,
 * how the camera frames the scene, and an optional pass that reworks the
 * rendered pixels. The geometry never changes.
 */
import * as THREE from 'three';

const C = (h) => new THREE.Color(h);

/** Lambert-ish: cheap, flat, no specular. Reads as painted. */
const flat = (color, o = {}) => new THREE.MeshLambertMaterial({ color: C(color), ...o });
const std = (color, o = {}) => new THREE.MeshStandardMaterial({ color: C(color), ...o });
const toon = (color, o = {}) => new THREE.MeshToonMaterial({ color: C(color), ...o });
const basic = (color, o = {}) => new THREE.MeshBasicMaterial({ color: C(color), ...o });

function sunAndSky(scene, { sky, ground, sun, sunPos, intensity, ambient, shadows }) {
  scene.add(new THREE.HemisphereLight(C(sky), C(ground), ambient));
  const key = new THREE.DirectionalLight(C(sun), intensity);
  key.position.set(...sunPos);
  if (shadows) {
    key.castShadow = true;
    key.shadow.mapSize.set(2048, 2048);
    const d = 11;
    Object.assign(key.shadow.camera, { left: -d, right: d, top: d, bottom: -d, near: 1, far: 46 });
    key.shadow.bias = -0.0012;
    key.shadow.radius = 3;
  }
  scene.add(key);
  return key;
}

/** Vertical gradient backdrop, drawn as a texture on the scene background. */
function skyTexture(top, bottom) {
  const cv = document.createElement('canvas');
  cv.width = 4; cv.height = 256;
  const g = cv.getContext('2d');
  const grad = g.createLinearGradient(0, 0, 0, 256);
  grad.addColorStop(0, top);
  grad.addColorStop(1, bottom);
  g.fillStyle = grad;
  g.fillRect(0, 0, 4, 256);
  const t = new THREE.CanvasTexture(cv);
  t.colorSpace = THREE.SRGBColorSpace;
  return t;
}

// ---------------------------------------------------------------------------

export const STYLES = [
  {
    id: 'toy',
    label: 'Toy',
    caption: ['flat shading, saturated', 'no specular, one sun'],
    camera: { fov: 38, dist: 13.5, pitch: 0.46, spin: 0.055, ortho: false },
    shadows: true,
    ui: 'dark',
    setup(scene) {
      scene.background = skyTexture('#5FCBEF', '#BFE9F7');
      scene.fog = new THREE.Fog(C('#BFEDF8'), 26, 64);
      sunAndSky(scene, {
        sky: '#CFF3FB', ground: '#3B9E62', sun: '#FFF6E0',
        sunPos: [9, 15, 7], intensity: 2.1, ambient: 1.15, shadows: true,
      });
    },
    material(role) {
      const m = {
        ground: '#C08B4E', grass: '#4CC96A', sand: '#F2D9A0', water: '#1799C4',
        wood: '#A5643C', roof: '#F2664F', slideDeck: '#F2664F',
        metalA: '#5A6472', metalB: '#4E8FE0', rope: '#8A6A4A',
        plastic: '#F2C14E', plasticA: '#F2664F', plasticB: '#4E8FE0', plasticC: '#F2C14E',
        trunk: '#8A5A34', foliage: '#3BA85A', rock: '#8D9AA5', lamp: '#FFF2C4',
        flower: '#F5E14A',
      }[role] || '#CCCCCC';
      if (role === 'water') return flat(m, { transparent: true, opacity: 0.9 });
      if (role === 'lamp') return basic(m);
      return flat(m);
    },
  },

  {
    id: 'aero',
    label: 'Frutiger Aero',
    caption: ['gloss, transmission, bloom', 'aqua and lime, circa 2007'],
    camera: { fov: 36, dist: 13.5, pitch: 0.4, spin: 0.045, ortho: false },
    shadows: true,
    bubbles: true,
    post: 'bloom',
    postOpts: { amount: 0.22 },
    ui: 'light',
    setup(scene, renderer) {
      scene.background = skyTexture('#12A8DE', '#BFEAF7');
      scene.fog = new THREE.Fog(C('#A9DEF0'), 30, 74);
      sunAndSky(scene, {
        sky: '#CDEBF7', ground: '#2E8F5A', sun: '#FFF8EC',
        sunPos: [8, 16, 9], intensity: 1.45, ambient: 0.5, shadows: true,
      });
      // A pre-filtered probe gives the gloss something to reflect, which is
      // most of what makes this look expensive rather than merely shiny.
      const pmrem = new THREE.PMREMGenerator(renderer);
      const env = new THREE.Scene();
      env.add(new THREE.Mesh(new THREE.SphereGeometry(40, 12, 8),
        new THREE.MeshBasicMaterial({ color: C('#8FE4FF'), side: THREE.BackSide })));
      const panel = (w, h, c, x, y, z) => {
        const m = new THREE.Mesh(new THREE.PlaneGeometry(w, h), new THREE.MeshBasicMaterial({ color: C(c) }));
        m.position.set(x, y, z); m.lookAt(0, 0, 0); env.add(m);
      };
      panel(26, 26, '#FFFFFF', 8, 20, 6);
      panel(18, 18, '#B6FF4D', -12, 4, -10);
      scene.environment = pmrem.fromScene(env, 0.03).texture;
      pmrem.dispose();
    },
    material(role) {
      const glossy = (c, o = {}) => std(c, {
        roughness: 0.16, metalness: 0, clearcoat: 1, envMapIntensity: 1.4, ...o,
      });
      switch (role) {
        case 'water': return new THREE.MeshPhysicalMaterial({
          color: C('#0578AE'), roughness: 0.06, metalness: 0.1, transmission: 0.25,
          thickness: 2.2, transparent: true, opacity: 0.95, envMapIntensity: 2.2,
        });
        case 'grass': return glossy('#57D93A');
        case 'ground': return glossy('#C79A5E');
        case 'sand': return glossy('#FFE9B0');
        case 'roof': case 'slideDeck': return glossy('#22C9E8');
        case 'wood': return glossy('#EFF6FA');
        case 'metalA': return std('#DCEBF2', { roughness: 0.1, metalness: 0.9, envMapIntensity: 2 });
        case 'metalB': return std('#8FE4FF', { roughness: 0.08, metalness: 0.7, envMapIntensity: 2 });
        case 'plastic': case 'plasticC': return glossy('#B6FF4D');
        case 'plasticA': return glossy('#FF7A3D');
        case 'plasticB': return glossy('#2AA7F0');
        case 'foliage': return glossy('#3FCF6A');
        case 'trunk': return glossy('#B98A5E');
        case 'flower': return glossy('#F2E356');
        case 'rock': return glossy('#CFE0E8');
        case 'rope': return std('#C8D8E0', { roughness: 0.4 });
        case 'lamp': return basic('#FFFFFF');
        default: return glossy('#DDDDDD');
      }
    },
  },

  {
    id: 'blueprint',
    label: 'Blueprint',
    caption: ['edge extraction, orthographic', 'drafting linework'],
    camera: { fov: 34, dist: 13, pitch: 0.4, spin: 0.03, ortho: true },
    shadows: false,
    edges: { color: '#A9DEFF', opacity: 0.95 },
    ui: 'dark',
    setup(scene) {
      scene.background = C('#0A2A5E');
      scene.fog = null;
      scene.add(new THREE.AmbientLight(0xffffff, 1));
      const grid = new THREE.GridHelper(60, 60, C('#1E4E8C'), C('#143A6B'));
      grid.position.y = -0.74;
      scene.add(grid);
    },
    // Draws nothing but still writes depth, which gives hidden line removal:
    // edges behind a solid are occluded, as they would be on a drawing.
    material() { return new THREE.MeshBasicMaterial({ colorWrite: false }); },
  },

  {
    id: 'hologram',
    label: 'Hologram',
    caption: ['additive shells, scanlines', 'everything visible at once'],
    camera: { fov: 38, dist: 13.5, pitch: 0.38, spin: 0.07, ortho: false },
    shadows: false,
    edges: { color: '#8CFFF0', opacity: 0.55 },
    post: 'scanlines',
    ui: 'dark',
    setup(scene) {
      scene.background = C('#03080C');
      scene.fog = new THREE.Fog(C('#03080C'), 14, 44);
      scene.add(new THREE.AmbientLight(0xffffff, 1));
    },
    material(role) {
      const c = role === 'water' ? '#0A7FA0' : '#25D8C8';
      return new THREE.MeshBasicMaterial({
        color: C(c), transparent: true, opacity: role === 'water' ? 0.1 : 0.055,
        blending: THREE.AdditiveBlending, depthWrite: false, side: THREE.DoubleSide,
      });
    },
  },

  {
    id: 'ascii',
    label: 'ASCII',
    caption: ['luminance ramp, 128 columns', 'rendered, then read as text'],
    camera: { fov: 40, dist: 13, pitch: 0.46, spin: 0.05, ortho: false },
    shadows: false,
    post: 'ascii',
    grid: { cols: 128 },
    ui: 'dark',
    setup(scene) {
      scene.background = C('#05070A');
      scene.fog = null;
      sunAndSky(scene, {
        sky: '#FFFFFF', ground: '#202020', sun: '#FFFFFF',
        sunPos: [8, 14, 6], intensity: 2.6, ambient: 0.5, shadows: false,
      });
    },
    material(role) {
      // Only luminance survives the pass, so the palette is a grey ramp chosen
      // to separate the parts once they become characters.
      const g = {
        water: '#000000', ground: '#4A4A4A', grass: '#6A6A6A', sand: '#D8D8D8',
        wood: '#7A7A7A', roof: '#A0A0A0', slideDeck: '#A0A0A0',
        metalA: '#909090', metalB: '#B0B0B0', rope: '#606060',
        plastic: '#D0D0D0', plasticA: '#C0C0C0', plasticB: '#8A8A8A', plasticC: '#D8D8D8',
        trunk: '#5A5A5A', foliage: '#707070', rock: '#989898', lamp: '#FFFFFF',
        flower: '#E8E8E8',
      }[role] || '#808080';
      if (role === 'water') return basic('#000000');
      return flat(g);
    },
  },

  {
    id: 'pixel',
    label: 'Pixel',
    caption: ['240 x 135, 5-bit palette', 'ordered dither'],
    camera: { fov: 38, dist: 13.5, pitch: 0.46, spin: 0.05, ortho: false },
    shadows: true,
    post: 'pixel',
    renderScale: 0.2,
    ui: 'dark',
    setup(scene) {
      scene.background = skyTexture('#4FC7E8', '#BDEEF6');
      scene.fog = null;
      sunAndSky(scene, {
        sky: '#CFF3FB', ground: '#3B9E62', sun: '#FFF3D6',
        sunPos: [9, 14, 7], intensity: 2.2, ambient: 1.0, shadows: true,
      });
    },
    material(role) {
      const m = {
        ground: '#B5793C', grass: '#49C25E', sand: '#EFD79B', water: '#2BB6DE',
        wood: '#96562F', roof: '#E4523F', slideDeck: '#E4523F',
        metalA: '#59616E', metalB: '#3F83D6', rope: '#7E5F3F',
        plastic: '#EFBA3F', plasticA: '#E4523F', plasticB: '#3F83D6', plasticC: '#EFBA3F',
        trunk: '#7E4E2C', foliage: '#33A04F', rock: '#87939E', lamp: '#FFF0B8',
        flower: '#EFD84A',
      }[role] || '#BBBBBB';
      return flat(m);
    },
  },

  {
    id: 'riso',
    label: 'Risograph',
    caption: ['two inks, halftone screen', 'misregistered on purpose'],
    camera: { fov: 36, dist: 13.5, pitch: 0.46, spin: 0.04, ortho: false },
    shadows: true,
    post: 'riso',
    postOpts: { inks: [
      { color: [204, 255, 0], angle: 0.26, offset: [1.7, -1.2], cell: 4.6, gamma: 1.8 },
      { color: [26, 26, 32], angle: 1.02, offset: [-1.3, 1.5], cell: 3.8, gamma: 2.9 },
    ] },
    renderScale: 0.45,
    ui: 'light',
    setup(scene) {
      scene.background = C('#F2EFE4');
      scene.fog = null;
      sunAndSky(scene, {
        sky: '#FFFFFF', ground: '#8A8A8A', sun: '#FFFFFF',
        sunPos: [7, 13, 8], intensity: 2.3, ambient: 0.9, shadows: true,
      });
    },
    material(role) {
      const g = {
        water: '#FAF7EE', ground: '#9E9888', grass: '#C6C0AE', sand: '#F6F0DF',
        wood: '#7C7668', roof: '#5A5548', slideDeck: '#5A5548',
        metalA: '#9A9484', metalB: '#7E7869', rope: '#A9A392',
        plastic: '#6E6859', plasticA: '#5F5A4C', plasticB: '#8A8474', plasticC: '#6E6859',
        trunk: '#7C7668', foliage: '#9C9684', rock: '#A8A292', lamp: '#FFFFFF',
        flower: '#CFC9B6',
      }[role] || '#A0A0A0';
      return flat(g);
    },
  },

  {
    id: 'noir',
    label: 'Noir',
    caption: ['one light, long shadows', 'concrete and nothing else'],
    camera: { fov: 32, dist: 14, pitch: 0.32, spin: 0.035, ortho: false },
    shadows: true,
    ui: 'dark',
    setup(scene) {
      scene.background = C('#0C0C0E');
      scene.fog = new THREE.Fog(C('#0C0C0E'), 16, 52);
      scene.add(new THREE.HemisphereLight(C('#3A4048'), C('#000000'), 0.35));
      const key = new THREE.DirectionalLight(C('#FFFFFF'), 3.2);
      key.position.set(-7, 9, 5);
      key.castShadow = true;
      key.shadow.mapSize.set(2048, 2048);
      const d = 12;
      Object.assign(key.shadow.camera, { left: -d, right: d, top: d, bottom: -d, near: 1, far: 46 });
      key.shadow.bias = -0.0012;
      scene.add(key);
    },
    material(role) {
      if (role === 'lamp') return basic('#FFFFFF');
      if (role === 'water') return std('#0A0A0C', { roughness: 0.18, metalness: 0.5 });
      const g = {
        ground: '#2A2A2C', grass: '#38383A', sand: '#6E6E70', wood: '#4A4A4C',
        roof: '#5A5A5C', slideDeck: '#5A5A5C', metalA: '#7A7A7E', metalB: '#6A6A6E',
        rope: '#3A3A3C', plastic: '#5E5E60', plasticA: '#5E5E60', plasticB: '#4E4E50',
        plasticC: '#5E5E60', trunk: '#333335', foliage: '#3E3E40', rock: '#606064',
        flower: '#8A8A8E',
      }[role] || '#4A4A4C';
      return std(g, { roughness: 0.95, metalness: 0.02 });
    },
  },
];

export const byId = (id) => STYLES.find((s) => s.id === id) || STYLES[0];

// Minimalistic world map outlines - simplified polygons [lng, lat][]
// Designed for dark "neural" aesthetic rendering.

/* Canvas2D doesn't resolve CSS variables. We read --text-muted at draw
   time so the map paints in a tone that reads on both the sand light
   background and the warm-grey dark background. accent-rgb is too low-
   contrast on light mode at the alphas this map uses. */
function readMutedHex(): string {
  if (typeof window === 'undefined') return '#7A7872';
  const raw = getComputedStyle(document.documentElement)
    .getPropertyValue('--text-muted')
    .trim();
  return raw || '#7A7872';
}
function hexToRgb(hex: string): string {
  const m = hex.replace('#', '').match(/.{1,2}/g);
  if (!m || m.length < 3) return '122, 120, 114';
  const [r, g, b] = m.slice(0, 3).map((s) => parseInt(s, 16));
  return `${r}, ${g}, ${b}`;
}
const accentRgb = (alpha: number) => `rgba(${hexToRgb(readMutedHex())}, ${alpha})`;

export interface MapRegion {
  name: string;
  paths: [number, number][][]; // array of polygons, each polygon = array of [lng, lat]
  detail?: boolean; // higher-detail region (US)
}

// ── US States outline (simplified) ──
const US_OUTLINE: [number, number][] = [
  [-124.7, 48.4], [-123.0, 48.4], [-122.8, 48.1], [-122.4, 47.6],
  [-122.4, 46.8], [-121.0, 46.0], [-119.5, 46.0], [-117.0, 46.0],
  [-116.0, 46.0], [-114.0, 46.6], [-112.5, 44.5], [-111.0, 44.5],
  [-111.0, 42.0], [-111.0, 41.0], [-109.0, 41.0], [-109.0, 37.0],
  [-109.0, 31.3], [-108.2, 31.3], [-106.6, 31.8], [-104.0, 30.6],
  [-103.0, 29.0], [-101.4, 29.8], [-99.5, 27.5], [-97.1, 25.9],
  [-96.0, 28.2], [-94.5, 29.4], [-93.8, 29.7], [-91.0, 29.1],
  [-89.5, 28.9], [-89.0, 29.1], [-88.8, 30.4], [-87.6, 30.3],
  [-85.0, 29.6], [-83.0, 28.4], [-81.0, 25.1], [-80.0, 25.8],
  [-80.4, 27.8], [-80.6, 28.8], [-81.2, 30.7], [-81.5, 31.2],
  [-80.8, 32.1], [-79.7, 33.2], [-78.5, 33.9], [-77.5, 34.5],
  [-75.5, 35.5], [-75.4, 37.0], [-75.6, 38.5], [-74.0, 39.0],
  [-73.9, 40.5], [-72.0, 41.0], [-71.0, 41.5], [-70.0, 41.7],
  [-69.9, 43.0], [-67.0, 44.5], [-67.0, 47.0], [-69.0, 47.4],
  [-71.0, 45.0], [-73.3, 45.0], [-75.0, 45.0], [-76.8, 43.6],
  [-79.0, 43.2], [-79.0, 42.5], [-82.5, 41.7], [-83.5, 41.7],
  [-84.8, 41.7], [-84.8, 42.5], [-82.4, 43.0], [-82.5, 45.0],
  [-83.5, 46.0], [-84.1, 46.5], [-85.0, 46.8], [-88.0, 46.0],
  [-89.5, 48.0], [-92.0, 48.6], [-95.0, 49.0], [-104.0, 49.0],
  [-116.0, 49.0], [-122.8, 49.0], [-124.7, 48.4],
];

// ── US internal borders (major state boundaries for neural grid feel) ──
const US_INTERNAL: [number, number][][] = [
  // Mississippi River (approximate)
  [[-89.5, 48.0], [-90.5, 46.5], [-91.0, 44.0], [-91.5, 42.0], [-90.5, 39.0], [-89.5, 37.0], [-89.5, 35.0], [-90.0, 33.0], [-91.0, 31.0], [-89.5, 29.0]],
  // Appalachian line
  [[-80.5, 36.0], [-80.0, 37.5], [-79.5, 39.0], [-78.5, 40.5], [-77.0, 42.0], [-75.5, 43.5]],
  // I-10 corridor (south)
  [[-118.0, 34.0], [-112.0, 33.4], [-106.6, 31.8], [-97.5, 29.5], [-94.0, 30.0], [-89.0, 30.3], [-85.0, 30.5], [-81.5, 30.5]],
  // I-80 corridor (north)
  [[-122.4, 37.8], [-119.0, 40.8], [-111.0, 41.0], [-104.8, 41.0], [-96.0, 41.2], [-87.6, 41.9], [-84.0, 41.7], [-80.0, 41.0]],
];

// ── Simplified continent outlines ──
const NORTH_AMERICA_OTHER: [number, number][][] = [
  // Canada (very simplified top)
  [[-140, 60], [-130, 55], [-124, 49], [-95, 49], [-88, 48], [-84, 46], [-76, 44], [-67, 47], [-60, 47], [-55, 52], [-58, 55], [-65, 60], [-75, 62], [-85, 63], [-95, 62], [-110, 60], [-120, 60], [-140, 60]],
  // Mexico (simplified)
  [[-117, 32.5], [-110, 31], [-106, 31.8], [-104, 30.5], [-99, 26], [-97, 26], [-96, 19.5], [-92, 18], [-90, 16], [-87, 18], [-87, 21.5], [-90, 21.5], [-97, 22.5], [-97, 25.8], [-105, 24], [-109, 23], [-110, 24.5], [-115, 29], [-117, 32.5]],
];

const SOUTH_AMERICA: [number, number][] = [
  [-80, 9], [-77, 8], [-73, 11], [-68, 12], [-63, 10], [-59, 8], [-52, 4], [-50, 0],
  [-50, -2], [-44, -2], [-41, -5], [-35, -8], [-35, -12], [-39, -15], [-40, -22],
  [-43, -23], [-48, -28], [-52, -33], [-53, -34], [-57, -37], [-65, -42],
  [-68, -47], [-70, -52], [-74, -50], [-72, -45], [-72, -40], [-71, -30],
  [-70, -18], [-75, -14], [-76, -6], [-80, -2], [-80, 2], [-78, 4], [-77, 8], [-80, 9],
];

const EUROPE: [number, number][] = [
  [-10, 36], [-6, 37], [-2, 36], [3, 37], [6, 43], [3, 43], [-2, 43], [-5, 43.5],
  [-9, 43], [-9, 38], [-10, 36],
];
const EUROPE_NORTH: [number, number][] = [
  [-5, 50], [2, 51], [5, 53], [9, 54], [12, 55], [10, 58], [5, 62], [10, 63],
  [15, 69], [25, 71], [30, 70], [30, 65], [27, 60], [24, 56], [21, 55],
  [14, 54], [14, 52], [10, 48], [7, 47], [2, 47], [-2, 48], [-5, 48], [-5, 50],
];

const AFRICA: [number, number][] = [
  [-17, 15], [-16, 12], [-8, 5], [4, 5], [10, 4], [10, 0], [12, -5],
  [30, -3], [40, -10], [35, -20], [32, -28], [28, -33], [18, -35],
  [16, -29], [12, -17], [12, -6], [10, 0], [1, 6], [-3, 5], [-8, 5],
  [-13, 10], [-17, 15],
];
const AFRICA_NORTH: [number, number][] = [
  [-17, 15], [-13, 20], [-17, 25], [-13, 28], [-5, 35], [-2, 36], [3, 37],
  [10, 37], [12, 33], [25, 32], [32, 31], [35, 30], [37, 28], [40, 15],
  [43, 12], [50, 12], [44, 10], [35, 8], [30, 5], [20, 5], [10, 4], [4, 5],
  [-3, 5], [-8, 5], [-13, 10], [-17, 15],
];

const ASIA_OUTLINE: [number, number][] = [
  [30, 70], [40, 68], [60, 70], [80, 72], [100, 72], [120, 70], [140, 65],
  [150, 60], [155, 55], [142, 47], [135, 43], [130, 33], [120, 22],
  [110, 20], [105, 10], [100, 2], [103, 1], [110, -7], [115, -8],
  [120, -9], [115, -8], [105, 5], [100, 13], [98, 16], [92, 22],
  [88, 22], [80, 15], [77, 8], [73, 15], [68, 24], [62, 25], [56, 26],
  [51, 24], [48, 29], [45, 30], [40, 37], [30, 40], [27, 41], [30, 43],
  [40, 43], [42, 45], [50, 45], [55, 50], [60, 55], [50, 58], [40, 60],
  [30, 65], [30, 70],
];

const AUSTRALIA: [number, number][] = [
  [115, -20], [115, -25], [117, -35], [125, -35], [132, -32],
  [137, -35], [145, -39], [150, -38], [153, -28], [150, -23],
  [145, -15], [140, -12], [135, -12], [130, -13], [125, -15],
  [120, -18], [115, -20],
];

// ── Major shipping lanes / trade routes (for neural network feel) ──
export const TRADE_ROUTES: [number, number][][] = [
  // Trans-Pacific
  [[-122, 37], [-140, 35], [-160, 30], [-180, 25], [170, 22], [140, 25], [120, 22]],
  // Trans-Atlantic
  [[-74, 40], [-50, 42], [-30, 45], [-10, 48], [0, 51]],
  // Panama Canal route
  [[-80, 9], [-82, 10], [-90, 15], [-95, 25]],
  // US East-West internal
  [[-122, 37], [-105, 39], [-90, 39], [-80, 40], [-74, 40]],
];

// ── Key global ports / hubs ──
export interface MapMarker {
  name: string;
  lng: number;
  lat: number;
}
export const GLOBAL_PORTS: MapMarker[] = [
  { name: 'Shanghai', lng: 121.5, lat: 31.2 },
  { name: 'Singapore', lng: 103.8, lat: 1.3 },
  { name: 'Rotterdam', lng: 4.5, lat: 51.9 },
  { name: 'Dubai', lng: 55.3, lat: 25.3 },
  { name: 'Panama', lng: -79.5, lat: 9.0 },
  { name: 'Tokyo', lng: 139.7, lat: 35.7 },
  { name: 'Sydney', lng: 151.2, lat: -33.9 },
];

export const WORLD_REGIONS: MapRegion[] = [
  { name: 'US', paths: [US_OUTLINE], detail: true },
  { name: 'US-Internal', paths: US_INTERNAL, detail: true },
  { name: 'Canada-Mexico', paths: NORTH_AMERICA_OTHER },
  { name: 'South America', paths: [SOUTH_AMERICA] },
  { name: 'Europe-South', paths: [EUROPE] },
  { name: 'Europe-North', paths: [EUROPE_NORTH] },
  { name: 'Africa', paths: [AFRICA] },
  { name: 'Africa-North', paths: [AFRICA_NORTH] },
  { name: 'Asia', paths: [ASIA_OUTLINE] },
  { name: 'Australia', paths: [AUSTRALIA] },
];

// ── Projection helpers ──

/** Equirectangular projection: convert [lng, lat] to canvas [x, y] */
export function projectToCanvas(
  lng: number,
  lat: number,
  canvasW: number,
  canvasH: number,
  bounds?: { minLng: number; maxLng: number; minLat: number; maxLat: number },
  padding = 0.1,
): { x: number; y: number } {
  const b = bounds || { minLng: -180, maxLng: 180, minLat: -60, maxLat: 75 };
  const lngRange = b.maxLng - b.minLng;
  const latRange = b.maxLat - b.minLat;
  const pad = Math.max(lngRange, latRange) * padding;
  const x = ((lng - b.minLng + pad) / (lngRange + pad * 2)) * canvasW;
  const y = ((b.maxLat - lat + pad) / (latRange + pad * 2)) * canvasH;
  return { x, y };
}

/** US-focused bounds for detailed view */
export const US_BOUNDS = { minLng: -128, maxLng: -65, minLat: 23, maxLat: 50 };

/** World bounds */
export const WORLD_BOUNDS = { minLng: -180, maxLng: 180, minLat: -60, maxLat: 75 };

/** Draw map on canvas context */
export function drawWorldMap(
  ctx: CanvasRenderingContext2D,
  w: number,
  h: number,
  bounds: typeof WORLD_BOUNDS,
  options?: { focused?: boolean },
) {
  const proj = (lng: number, lat: number) => projectToCanvas(lng, lat, w, h, bounds, 0.05);
  const focused = options?.focused ?? false;

  // Subtle grid lines
  ctx.save();
  ctx.strokeStyle = accentRgb(0.10);
  ctx.lineWidth = 0.5;
  for (let lng = -180; lng <= 180; lng += 30) {
    const p1 = proj(lng, -60);
    const p2 = proj(lng, 75);
    ctx.beginPath();
    ctx.moveTo(p1.x, p1.y);
    ctx.lineTo(p2.x, p2.y);
    ctx.stroke();
  }
  for (let lat = -60; lat <= 75; lat += 15) {
    const p1 = proj(-180, lat);
    const p2 = proj(180, lat);
    ctx.beginPath();
    ctx.moveTo(p1.x, p1.y);
    ctx.lineTo(p2.x, p2.y);
    ctx.stroke();
  }
  ctx.restore();

  // Draw regions
  for (const region of WORLD_REGIONS) {
    const isUS = region.name === 'US';
    const isInternal = region.name === 'US-Internal';

    ctx.save();
    for (const polygon of region.paths) {
      ctx.beginPath();
      polygon.forEach(([lng, lat], i) => {
        const p = proj(lng, lat);
        if (i === 0) ctx.moveTo(p.x, p.y);
        else ctx.lineTo(p.x, p.y);
      });

      if (isUS) {
        // US fill
        ctx.fillStyle = focused
          ? accentRgb(0.18)
          : accentRgb(0.10);
        ctx.fill();
        ctx.strokeStyle = focused
          ? accentRgb(0.50)
          : accentRgb(0.30);
        ctx.lineWidth = focused ? 1.5 : 1;
        ctx.stroke();
      } else if (isInternal) {
        ctx.strokeStyle = accentRgb(0.18);
        ctx.lineWidth = 0.5;
        ctx.stroke();
      } else {
        // Other continents - very subtle
        ctx.fillStyle = accentRgb(0.05);
        ctx.fill();
        ctx.strokeStyle = accentRgb(0.18);
        ctx.lineWidth = 0.5;
        ctx.stroke();
      }
    }
    ctx.restore();
  }

  // Trade routes - dotted neural lines
  ctx.save();
  ctx.setLineDash([2, 6]);
  ctx.strokeStyle = 'rgba(167, 139, 250, 0.06)';
  ctx.lineWidth = 0.5;
  for (const route of TRADE_ROUTES) {
    ctx.beginPath();
    route.forEach(([lng, lat], i) => {
      const p = proj(lng, lat);
      if (i === 0) ctx.moveTo(p.x, p.y);
      else ctx.lineTo(p.x, p.y);
    });
    ctx.stroke();
  }
  ctx.setLineDash([]);
  ctx.restore();

  // Global port markers
  ctx.save();
  for (const port of GLOBAL_PORTS) {
    const p = proj(port.lng, port.lat);
    ctx.beginPath();
    ctx.arc(p.x, p.y, 2, 0, Math.PI * 2);
    ctx.fillStyle = accentRgb(0.15);
    ctx.fill();
  }
  ctx.restore();
}

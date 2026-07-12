import { useEffect, useRef } from 'react';
import './bigDipperCanvas.css';

interface Point {
  x: number;
  y: number;
}

interface BgStar {
  x: number;
  y: number;
  r: number;
  a: number;
}

// Normalized (0-1) Big Dipper (Odin's Wagon) star positions, ported
// verbatim from odin_login.html: handle tip -> handle -> bowl corner ->
// around the bowl -> back to corner, in a FIXED sequential trace order
// (not a random walk).
const STARS_NORM: Point[] = [
  { x: 0.051, y: 0.071 }, // A - handle tip
  { x: 0.276, y: 0.145 }, // B
  { x: 0.376, y: 0.311 }, // C
  { x: 0.518, y: 0.51 }, // D - bowl corner (handle meets bowl)
  { x: 0.476, y: 0.74 }, // E
  { x: 0.727, y: 0.88 }, // F
  { x: 0.849, y: 0.684 }, // G
];
const PATH: Array<[number, number]> = [[0, 1], [1, 2], [2, 3], [3, 4], [4, 5], [5, 6], [6, 3]];

const GREY = '#3A4048';
const GOLD = '#E8B76A';
const TRACE_SPEED = 0.006;
const HOLD_FRAMES = 110; // ~1.8s hold on the fully-lit dipper at 60fps

function lerp(a: number, b: number, k: number): number {
  return a + (b - a) * k;
}

function mixColor(c1: string, c2: string, k: number): string {
  const p1 = c1.match(/\w\w/g)!.map((h) => parseInt(h, 16));
  const p2 = c2.match(/\w\w/g)!.map((h) => parseInt(h, 16));
  const m = p1.map((v, i) => Math.round(lerp(v, p2[i], k)));
  return `rgb(${m[0]},${m[1]},${m[2]})`;
}

// Big Dipper constellation trace — ported directly from odin_login.html's
// canvas script, not reinterpreted. A real, tuned animation: segments
// trace sequentially in gold, hold fully lit, then fade back to grey and
// retrace, on a loop. Respects prefers-reduced-motion with a static
// fully-lit snapshot instead of the animation loop.
//
// Shared between Login and Signup (identical on both screens per the
// original prototype design) — extracted so neither page has to
// duplicate this ~150-line animation.
export function BigDipperCanvas() {
  const paneRef = useRef<HTMLDivElement>(null);
  const staticCanvasRef = useRef<HTMLCanvasElement>(null);
  const glowCanvasRef = useRef<HTMLCanvasElement>(null);

  useEffect(() => {
    const pane = paneRef.current;
    const staticCanvas = staticCanvasRef.current;
    const glowCanvas = glowCanvasRef.current;
    if (!pane || !staticCanvas || !glowCanvas) return;

    const sctx = staticCanvas.getContext('2d');
    const gctx = glowCanvas.getContext('2d');
    if (!sctx || !gctx) return;

    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    let width = 0;
    let height = 0;
    let stars: Point[] = [];
    const bgStars: BgStar[] = [];

    let edgeIndex = 0;
    let t = 0;
    let animState: 'tracing' | 'holding' | 'fading' = 'tracing';
    let holdTimer = 0;
    let fadeK = 0; // 0 = fully gold (just completed), 1 = fully faded back to grey
    let rafId = 0;

    function drawStaticStars() {
      sctx!.clearRect(0, 0, width, height);
      bgStars.forEach((s) => {
        sctx!.beginPath();
        sctx!.arc(s.x, s.y, s.r, 0, Math.PI * 2);
        sctx!.fillStyle = `rgba(200, 210, 225, ${s.a})`;
        sctx!.fill();
      });
    }

    function resize() {
      width = pane!.clientWidth;
      height = pane!.clientHeight;
      [staticCanvas, glowCanvas].forEach((c) => {
        c!.width = width * dpr;
        c!.height = height * dpr;
        c!.getContext('2d')!.setTransform(dpr, 0, 0, dpr, 0, 0);
      });
      stars = STARS_NORM.map((n) => ({ x: n.x * width, y: n.y * height }));
      if (bgStars.length === 0) {
        // Decorative background starfield — static, unrelated to the
        // constellation graph itself, just sets the night-sky context.
        for (let i = 0; i < 90; i++) {
          bgStars.push({
            x: Math.random() * width,
            y: Math.random() * height,
            r: Math.random() * 1.1 + 0.3,
            a: Math.random() * 0.5 + 0.15,
          });
        }
      }
      drawStaticStars();
    }

    function drawEdge(a: Point, b: Point, color: string, glow: boolean) {
      gctx!.beginPath();
      gctx!.moveTo(a.x, a.y);
      gctx!.lineTo(b.x, b.y);
      gctx!.strokeStyle = color;
      gctx!.lineWidth = 1.4;
      if (glow) {
        gctx!.shadowColor = GOLD;
        gctx!.shadowBlur = 6;
      } else {
        gctx!.shadowBlur = 0;
      }
      gctx!.stroke();
      gctx!.shadowBlur = 0;
    }

    function drawStar(p: Point, color: string, glow: boolean) {
      if (glow) {
        const halo = gctx!.createRadialGradient(p.x, p.y, 0, p.x, p.y, 14);
        halo.addColorStop(0, 'rgba(232, 183, 106, 0.45)');
        halo.addColorStop(1, 'rgba(232, 183, 106, 0)');
        gctx!.fillStyle = halo;
        gctx!.beginPath();
        gctx!.arc(p.x, p.y, 14, 0, Math.PI * 2);
        gctx!.fill();
      }
      gctx!.beginPath();
      gctx!.arc(p.x, p.y, 4, 0, Math.PI * 2);
      gctx!.fillStyle = color;
      gctx!.fill();
    }

    function drawFrame() {
      gctx!.clearRect(0, 0, width, height);

      const completedColor = mixColor('E8B76A', '3A4048', fadeK);
      const completedGlow = fadeK < 0.5;

      PATH.forEach((edge, i) => {
        const [ai, bi] = edge;
        const a = stars[ai];
        const b = stars[bi];
        if (i < edgeIndex) {
          drawEdge(a, b, completedColor, completedGlow);
        } else if (i === edgeIndex && animState === 'tracing') {
          const mx = lerp(a.x, b.x, t);
          const my = lerp(a.y, b.y, t);
          drawEdge(a, b, GREY, false);
          drawEdge(a, { x: mx, y: my }, GOLD, true);
        } else {
          drawEdge(a, b, GREY, false);
        }
      });

      const reachedUpTo = animState === 'tracing' ? edgeIndex : PATH.length;
      stars.forEach((s, idx) => {
        const isReached =
          PATH.slice(0, reachedUpTo).some((e) => e.includes(idx)) ||
          (animState === 'tracing' && PATH[edgeIndex] && PATH[edgeIndex][0] === idx);
        drawStar(s, isReached ? completedColor : GREY, isReached && completedGlow);
      });

      if (animState === 'tracing') {
        const [ai, bi] = PATH[edgeIndex];
        const a = stars[ai];
        const b = stars[bi];
        const px = lerp(a.x, b.x, t);
        const py = lerp(a.y, b.y, t);
        const grad = gctx!.createRadialGradient(px, py, 0, px, py, 16);
        grad.addColorStop(0, 'rgba(251, 235, 207, 0.95)');
        grad.addColorStop(0.5, 'rgba(232, 183, 106, 0.3)');
        grad.addColorStop(1, 'rgba(232, 183, 106, 0)');
        gctx!.fillStyle = grad;
        gctx!.beginPath();
        gctx!.arc(px, py, 16, 0, Math.PI * 2);
        gctx!.fill();

        t += TRACE_SPEED;
        if (t >= 1) {
          t = 0;
          edgeIndex++;
          if (edgeIndex >= PATH.length) {
            animState = 'holding';
            holdTimer = 0;
          }
        }
      } else if (animState === 'holding') {
        holdTimer++;
        if (holdTimer > HOLD_FRAMES) {
          animState = 'fading';
        }
      } else if (animState === 'fading') {
        fadeK += 0.02;
        if (fadeK >= 1) {
          fadeK = 0;
          edgeIndex = 0;
          t = 0;
          animState = 'tracing';
        }
      }

      rafId = requestAnimationFrame(drawFrame);
    }

    window.addEventListener('resize', resize);
    resize();

    const reduceMotion = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    if (reduceMotion) {
      PATH.forEach(([ai, bi]) => drawEdge(stars[ai], stars[bi], GOLD, true));
      stars.forEach((s) => drawStar(s, GOLD, true));
    } else {
      rafId = requestAnimationFrame(drawFrame);
    }

    return () => {
      window.removeEventListener('resize', resize);
      cancelAnimationFrame(rafId);
    };
  }, []);

  return (
    <section className="graph-pane" aria-hidden="true" ref={paneRef}>
      <canvas ref={staticCanvasRef} />
      <canvas ref={glowCanvasRef} />
    </section>
  );
}

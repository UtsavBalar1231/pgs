import { mkdirSync, writeFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const colors = {
  ink: "#342c2a",
  orange: "#f45636",
  paper: "#fff4df",
  fold: "#ffbd9c",
  back: "#e6b898",
};
const rgba = (hex) => [
  ...hex
    .slice(1)
    .match(/../g)
    .map((n) => Number.parseInt(n, 16) / 255),
  1,
];
const fixed = (k) => ({ a: 0, k });
const curves = {
  enter: [0.2, 0.75, 0.34, 0.94],
  settle: [0, 0.65, 0.51, 0.99],
  travel: [1, 0.49, 0, 0.55],
};
function keys(points, behavior = "settle") {
  const [x1, y1, x2, y2] = curves[behavior];
  return {
    a: 1,
    k: points.map(([t, value], index) => ({
      t,
      s: Array.isArray(value) ? value : [value],
      ...(index + 1 < points.length
        ? { o: { x: [x1], y: [y1] }, i: { x: [x2], y: [y2] } }
        : {}),
    })),
  };
}
function geometry(commands) {
  const v = [],
    i = [],
    o = [];
  let closed = false;
  for (const [command, ...p] of commands) {
    if (command === "Z") {
      closed = true;
      continue;
    }
    if (command === "C") {
      const previous = v[v.length - 1];
      o[o.length - 1] = [p[0] - previous[0], p[1] - previous[1]];
      v.push([p[4], p[5]]);
      i.push([p[2] - p[4], p[3] - p[5]]);
      o.push([0, 0]);
    } else {
      v.push(p);
      i.push([0, 0]);
      o.push([0, 0]);
    }
  }
  return { v, i, o, c: closed };
}
const paths = {
  card: [
    ["M", 150, 114],
    ["L", 310, 114],
    ["C", 315, 114, 318, 116, 322, 120],
    ["L", 372, 170],
    ["C", 376, 174, 378, 178, 378, 184],
    ["L", 378, 364],
    ["C", 378, 378, 369, 388, 355, 388],
    ["L", 150, 388],
    ["C", 136, 388, 126, 378, 126, 364],
    ["L", 126, 138],
    ["C", 126, 124, 136, 114, 150, 114],
    ["Z"],
  ],
  fold: [
    ["M", 314, 116],
    ["L", 314, 162],
    ["C", 314, 173, 321, 180, 332, 180],
    ["L", 375, 180],
    ["Z"],
  ],
  seam: [
    ["M", 314, 120],
    ["L", 314, 161],
    ["C", 314, 173, 321, 180, 333, 180],
    ["L", 374, 180],
  ],
  stem: [
    ["M", 207, 236],
    ["L", 207, 326],
  ],
  branch: [
    ["M", 207, 262],
    ["L", 251, 262],
    ["C", 283, 262, 300, 278, 300, 309],
  ],
  diff1: [
    ["M", 165, 164],
    ["L", 220, 164],
  ],
  diff2: [
    ["M", 165, 188],
    ["L", 254, 188],
  ],
};
const svgPath = (commands) =>
  commands.map(([c, ...p]) => `${c}${p.join(" ")}`).join(" ");
const transform = {
  ty: "tr",
  p: fixed([0, 0]),
  a: fixed([0, 0]),
  s: fixed([100, 100]),
  r: fixed(0),
  o: fixed(100),
  sk: fixed(0),
  sa: fixed(0),
};
function shape(path) {
  return { ty: "sh", ks: fixed(geometry(path)) };
}
function circle(x, y, size) {
  return { ty: "el", p: fixed([x, y]), s: fixed([size, size]) };
}
function paint(fill, stroke, width = 7) {
  return [
    ...(fill
      ? [
          {
            ty: "fl",
            c: { ...fixed(rgba(colors[fill])), sid: fill },
            o: fixed(100),
            r: 1,
          },
        ]
      : []),
    ...(stroke
      ? [
          {
            ty: "st",
            c: { ...fixed(rgba(colors[stroke])), sid: stroke },
            o: fixed(100),
            w: fixed(width),
            lc: 2,
            lj: 2,
            ml: 4,
          },
        ]
      : []),
  ];
}
function group(name, item, fill, stroke, extra = [], pose = {}) {
  return {
    ty: "gr",
    nm: name,
    it: [item, ...paint(fill, stroke), ...extra, { ...transform, ...pose }],
  };
}
function trimmed(name, path, start, end, width = 12) {
  const g = group(name, shape(path), null, "paper", [
    {
      ty: "tm",
      s: fixed(0),
      e: keys([
        [0, 0],
        [start, 0],
        [end, 100],
      ]),
      o: fixed(0),
      m: 1,
    },
  ]);
  g.it.find((item) => item.ty === "st").w = fixed(width);
  return g;
}
function layer(name, groups, pose = {}, parent) {
  return {
    ddd: 0,
    ind: 0,
    ty: 4,
    nm: name,
    sr: 1,
    ...(parent ? { parent } : {}),
    ks: {
      o: fixed(100),
      r: fixed(0),
      p: fixed([256, 256, 0]),
      a: fixed([256, 256, 0]),
      s: fixed([100, 100, 100]),
      ...pose,
    },
    ao: 0,
    shapes: groups,
    ip: 0,
    op: 240,
    st: 0,
    bm: 0,
  };
}
const rest = (v) => fixed(v);
function makeAnimation() {
  const front = layer("Front patch / shuffle and settle", [], {
    p: keys(
      [
        [0, [250, 306, 0]],
        [14, [250, 306, 0]],
        [40, [256, 250, 0]],
        [52, [256, 256, 0]],
      ],
      "enter",
    ),
    r: keys([
      [0, 9],
      [14, 9],
      [40, -9],
      [54, -7],
    ]),
    o: keys([
      [0, 0],
      [14, 0],
      [21, 100],
    ]),
  });
  front.ty = 3;
  delete front.shapes;
  front.ind = 1;
  const content = [
    layer(
      "Selected hunk docks into commit",
      [
        group("Commit core", circle(300, 324, 12), "orange", null, [], {
          o: keys([
            [0, 0],
            [86, 0],
            [96, 100],
          ]),
        }),
        group(
          "Selected patch becomes a commit",
          {
            ty: "rc",
            p: fixed([0, 0]),
            s: keys(
              [
                [0, [72, 32]],
                [64, [72, 32]],
                [86, [40, 40]],
              ],
              "travel",
            ),
            r: keys([
              [0, 9],
              [64, 9],
              [86, 20],
            ]),
          },
          "paper",
          "ink",
          [],
          {
            p: keys(
              [
                [0, [414, 258]],
                [64, [414, 258]],
                [86, [300, 324]],
              ],
              "travel",
            ),
            s: keys([
              [0, [0, 0]],
              [56, [0, 0]],
              [64, [100, 100]],
              [88, [108, 108]],
              [98, [100, 100]],
            ]),
          },
        ),
      ],
      {},
      1,
    ),
    layer(
      "Path / chosen changes become a commit",
      [
        group("Head node", circle(207, 236, 38), "paper", null, [], {
          s: keys([
            [0, [0, 0]],
            [46, [0, 0]],
            [58, [100, 100]],
          ]),
          a: rest([207, 236]),
          p: rest([207, 236]),
        }),
        group("History node", circle(207, 326, 38), "paper", null, [], {
          s: keys([
            [0, [0, 0]],
            [65, [0, 0]],
            [77, [100, 100]],
          ]),
          a: rest([207, 326]),
          p: rest([207, 326]),
        }),
        trimmed("Mainline", paths.stem, 52, 72),
        trimmed("Selected branch", paths.branch, 65, 88),
      ],
      {},
      1,
    ),
    layer(
      "Patch header / two lines",
      [
        trimmed("Short line", paths.diff1, 34, 47, 10),
        trimmed("Long line", paths.diff2, 39, 55, 10),
      ],
      {},
      1,
    ),
    layer(
      "Fold / paper hinge",
      [
        group("Folded corner", shape(paths.fold), "fold", null),
        group("Crease", shape(paths.seam), null, "ink"),
      ],
      {
        o: keys([
          [0, 0],
          [26, 0],
          [38, 100],
        ]),
      },
      1,
    ),
    layer(
      "Orange patch / die cut silhouette",
      [group("Patch", shape(paths.card), "orange", "ink")],
      {},
      1,
    ),
  ];
  const back = [
    layer(
      "Middle patch / follow through",
      [group("Middle sheet", shape(paths.card), "paper", "ink")],
      {
        p: keys(
          [
            [0, [259, 326, 0]],
            [8, [259, 326, 0]],
            [30, [273, 269, 0]],
            [45, [271, 270, 0]],
          ],
          "enter",
        ),
        r: keys([
          [0, -6],
          [30, 2],
          [45, 1],
        ]),
        o: keys([
          [0, 0],
          [7, 0],
          [14, 100],
        ]),
      },
    ),
    layer(
      "Back patch / first beat",
      [group("Backing sheet", shape(paths.card), "back", "ink")],
      {
        p: keys(
          [
            [0, [272, 334, 0]],
            [24, [288, 282, 0]],
            [38, [285, 281, 0]],
          ],
          "enter",
        ),
        r: keys([
          [0, 12],
          [24, 9],
          [38, 8],
        ]),
        o: keys([
          [0, 0],
          [8, 100],
        ]),
      },
    ),
  ];
  for (const item of content) {
    const reveal = item.ks.o;
    item.ks.o =
      reveal.a === 1
        ? reveal
        : keys([
            [0, 0],
            [14, 0],
            [21, 100],
          ]);
  }
  const layers = [front, ...content, ...back].map((l, i) => ({
    ...l,
    ind: i + 1,
  }));
  return {
    v: "5.12.2",
    fr: 60,
    ip: 0,
    op: 240,
    w: 512,
    h: 512,
    nm: "pgs / staged patch",
    ddd: 0,
    assets: [],
    slots: Object.fromEntries(
      Object.entries(colors).map(([k, v]) => [k, { p: fixed(rgba(v)) }]),
    ),
    markers: [
      { tm: 0, cm: "shuffle", dr: 40 },
      { tm: 34, cm: "inspect", dr: 24 },
      { tm: 56, cm: "select and route", dr: 40 },
      { tm: 98, cm: "settled lockup", dr: 142 },
    ],
    layers,
  };
}
function staticSVG() {
  const card = (fill) =>
    `<path d="${svgPath(paths.card)}" fill="${colors[fill]}" stroke="${colors.ink}" stroke-width="7" stroke-linejoin="round"/>`;
  return `<svg xmlns="http://www.w3.org/2000/svg" width="512" height="512" viewBox="0 0 512 512"><title>pgs staged patch icon</title><g transform="translate(29 25) rotate(8 256 256)">${card("back")}</g><g transform="translate(15 14) rotate(1 256 256)">${card("paper")}</g><g transform="rotate(-7 256 256)">${card("orange")}<path d="${svgPath(paths.fold)}" fill="${colors.fold}"/><path d="${svgPath(paths.seam)}" fill="none" stroke="${colors.ink}" stroke-width="7" stroke-linecap="round"/><g fill="none" stroke="${colors.paper}" stroke-linecap="round"><path d="${svgPath(paths.diff1)} ${svgPath(paths.diff2)}" stroke-width="10"/><path d="${svgPath(paths.stem)} ${svgPath(paths.branch)}" stroke-width="12"/></g><g fill="${colors.paper}"><circle cx="207" cy="236" r="19"/><circle cx="207" cy="326" r="19"/><circle cx="300" cy="324" r="20" stroke="${colors.ink}" stroke-width="7"/></g><circle cx="300" cy="324" r="6" fill="${colors.orange}"/></g></svg>\n`;
}
const scene = resolve(root, "out/skottie-player/public/projects/pgs/scene-7");
const output = resolve(root, "out/staged-patch");
for (const dir of [scene, output]) mkdirSync(dir, { recursive: true });
const doc = makeAnimation();
for (const dir of [scene, output])
  writeFileSync(resolve(dir, "lottie.json"), JSON.stringify(doc) + "\n");
writeFileSync(
  resolve(scene, "controls.json"),
  JSON.stringify({
    controls: Object.keys(colors).map((sid) => ({
      sid,
      label: {
        ink: "Charcoal outline",
        orange: "Git orange",
        paper: "Warm paper",
        fold: "Fold highlight",
        back: "Backing paper",
      }[sid],
    })),
  }) + "\n",
);
writeFileSync(resolve(output, "pgs-staged-patch.svg"), staticSVG());
console.log("Wrote staged-patch study to scene-7 and out/staged-patch.");

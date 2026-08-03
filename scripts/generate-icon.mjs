// プレースホルダの小人ドット絵アイコンを生成する(依存パッケージなし)。
// 16x16 のドット絵を 64 倍に拡大した 1024x1024 の app-icon.png を書き出す。
// その後 `pnpm tauri icon` で src-tauri/icons/ 一式を生成すること。
// マイルストーン5 で本番アート(残量連動アニメーションフレーム)に差し替える。
import { deflateSync } from "node:zlib";
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const SCALE = 64;

const PALETTE = {
  ".": null,
  R: [0xd6, 0x49, 0x43, 0xff], // とんがり帽子
  r: [0xa8, 0x32, 0x38, 0xff], // 帽子の影
  S: [0xf6, 0xc9, 0xa4, 0xff], // 肌
  E: [0x3a, 0x2b, 0x27, 0xff], // 目
  N: [0xe0, 0x9c, 0x74, 0xff], // 鼻
  B: [0xef, 0xe6, 0xd8, 0xff], // ひげ
  U: [0x4a, 0x6f, 0xa5, 0xff], // 服
  F: [0x5b, 0x46, 0x32, 0xff], // 靴
};

const SPRITE = [
  ".......RR.......",
  "......RRRR......",
  "......RRRR......",
  ".....RRRRRR.....",
  ".....RRRRRR.....",
  "....RRRRRRRR....",
  "...RRRRRRRRRr...",
  "..rRRRRRRRRRRr..",
  "...SSSSSSSSSS...",
  "...SSESSSSESS...",
  "...SSSSNNSSSS...",
  "....BBBBBBBB....",
  "...BBBBBBBBBB...",
  "....UUUUUUUU....",
  "....UUUUUUUU....",
  "....FF....FF....",
];

function crc32(buf) {
  if (!crc32.table) {
    crc32.table = new Int32Array(256);
    for (let n = 0; n < 256; n++) {
      let c = n;
      for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1;
      crc32.table[n] = c;
    }
  }
  let crc = -1;
  for (const b of buf) crc = crc32.table[(crc ^ b) & 0xff] ^ (crc >>> 8);
  return (crc ^ -1) >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body));
  return Buffer.concat([len, body, crc]);
}

function encodePng(width, height, rgba) {
  const signature = Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]);
  const ihdr = Buffer.alloc(13);
  ihdr.writeUInt32BE(width, 0);
  ihdr.writeUInt32BE(height, 4);
  ihdr[8] = 8; // bit depth
  ihdr[9] = 6; // color type: RGBA
  const stride = width * 4 + 1;
  const raw = Buffer.alloc(stride * height);
  for (let y = 0; y < height; y++) {
    raw[y * stride] = 0; // filter: none
    rgba.copy(raw, y * stride + 1, y * width * 4, (y + 1) * width * 4);
  }
  return Buffer.concat([
    signature,
    chunk("IHDR", ihdr),
    chunk("IDAT", deflateSync(raw, { level: 9 })),
    chunk("IEND", Buffer.alloc(0)),
  ]);
}

const size = SPRITE.length * SCALE;
const rgba = Buffer.alloc(size * size * 4);
SPRITE.forEach((row, py) => {
  if (row.length !== SPRITE.length) {
    throw new Error(`SPRITE の ${py} 行目が ${SPRITE.length} 文字ではない: "${row}"`);
  }
  [...row].forEach((ch, px) => {
    const color = PALETTE[ch];
    if (ch !== "." && !color) throw new Error(`未定義の色: "${ch}"`);
    if (!color) return;
    for (let dy = 0; dy < SCALE; dy++) {
      const rowOffset = ((py * SCALE + dy) * size + px * SCALE) * 4;
      for (let dx = 0; dx < SCALE; dx++) {
        rgba.set(color, rowOffset + dx * 4);
      }
    }
  });
});

const out = join(dirname(fileURLToPath(import.meta.url)), "..", "app-icon.png");
writeFileSync(out, encodePng(size, size, rgba));
console.log(`wrote ${out} (${size}x${size})`);

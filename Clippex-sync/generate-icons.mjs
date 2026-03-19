import sharp from 'sharp';
import fs from 'fs';
import path from 'path';

const svgBuffer = fs.readFileSync('icon.svg');
const iconsDir = 'src-tauri/icons';

const sizes = [
  { name: '32x32.png', size: 32 },
  { name: '128x128.png', size: 128 },
  { name: '128x128@2x.png', size: 256 },
  { name: 'icon.png', size: 512 },
  { name: 'Square30x30Logo.png', size: 30 },
  { name: 'Square44x44Logo.png', size: 44 },
  { name: 'Square71x71Logo.png', size: 71 },
  { name: 'Square89x89Logo.png', size: 89 },
  { name: 'Square107x107Logo.png', size: 107 },
  { name: 'Square142x142Logo.png', size: 142 },
  { name: 'Square150x150Logo.png', size: 150 },
  { name: 'Square284x284Logo.png', size: 284 },
  { name: 'Square310x310Logo.png', size: 310 },
  { name: 'StoreLogo.png', size: 50 },
];

async function generate() {
  // Generate PNGs
  for (const { name, size } of sizes) {
    await sharp(svgBuffer)
      .resize(size, size)
      .png()
      .toFile(path.join(iconsDir, name));
    console.log(`Generated ${name} (${size}x${size})`);
  }

  // Generate ICO (256px PNG as base)
  const ico256 = await sharp(svgBuffer).resize(256, 256).png().toBuffer();
  const ico48 = await sharp(svgBuffer).resize(48, 48).png().toBuffer();
  const ico32 = await sharp(svgBuffer).resize(32, 32).png().toBuffer();
  const ico16 = await sharp(svgBuffer).resize(16, 16).png().toBuffer();

  // Simple ICO format with PNG entries
  const images = [ico16, ico32, ico48, ico256];
  const headerSize = 6 + images.length * 16;
  let dataOffset = headerSize;
  const entries = [];

  for (const img of images) {
    const size = img === ico256 ? 0 : Math.sqrt(img.length) > 255 ? 0 : 0;
    const dim = img === ico16 ? 16 : img === ico32 ? 32 : img === ico48 ? 48 : 0;
    entries.push({ dim, offset: dataOffset, data: img });
    dataOffset += img.length;
  }

  const ico = Buffer.alloc(dataOffset);
  // ICO header
  ico.writeUInt16LE(0, 0);     // Reserved
  ico.writeUInt16LE(1, 2);     // Type: ICO
  ico.writeUInt16LE(images.length, 4); // Count

  let entryOffset = 6;
  for (const entry of entries) {
    ico.writeUInt8(entry.dim, entryOffset);      // Width
    ico.writeUInt8(entry.dim, entryOffset + 1);   // Height
    ico.writeUInt8(0, entryOffset + 2);           // Color palette
    ico.writeUInt8(0, entryOffset + 3);           // Reserved
    ico.writeUInt16LE(1, entryOffset + 4);        // Color planes
    ico.writeUInt16LE(32, entryOffset + 6);       // Bits per pixel
    ico.writeUInt32LE(entry.data.length, entryOffset + 8);  // Size
    ico.writeUInt32LE(entry.offset, entryOffset + 12);      // Offset
    entry.data.copy(ico, entry.offset);
    entryOffset += 16;
  }

  fs.writeFileSync(path.join(iconsDir, 'icon.ico'), ico);
  console.log('Generated icon.ico');

  console.log('Done!');
}

generate().catch(console.error);

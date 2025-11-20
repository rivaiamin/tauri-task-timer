import { mkdir, cp } from 'fs/promises';
import { existsSync } from 'fs';
import { join } from 'path';

const distDir = 'dist';
const srcDir = 'src';

async function build() {
  try {
    // Create dist directory if it doesn't exist
    if (!existsSync(distDir)) {
      await mkdir(distDir, { recursive: true });
    }

    // Copy all files from src to dist
    await cp(srcDir, distDir, { recursive: true });

    console.log('✓ Build completed successfully!');
    console.log(`✓ Files copied to ${distDir}/`);
  } catch (error) {
    console.error('✗ Build failed:', error);
    process.exit(1);
  }
}

build();


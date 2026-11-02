// The file a launched browser writes its DevTools endpoint to, so scripts can attach to it.
import { mkdir, rm, writeFile } from 'node:fs/promises';
import { dirname } from 'node:path';

export async function writeEndpoint(params: { file: string; endpoint: string }): Promise<void> {
  await mkdir(dirname(params.file), { recursive: true });
  await writeFile(params.file, `${params.endpoint}\n`);
}

export async function removeEndpoint(file: string): Promise<void> {
  await rm(file, { force: true });
}

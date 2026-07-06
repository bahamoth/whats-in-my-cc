// vite.config.js/.d.ts 존재 게이트 (2026-07-06 실사고).
//
// vite는 .ts 설정을 직접 로드하므로 js 산출물이란 단계 자체가 없다 — 그런데
// 어떤 경로로든 vite.config.js가 생기면 vite 설정 탐색이 .ts보다 .js를 먼저
// 집어(공식 docs: config resolution order) `@` alias 없는 낡은 설정으로
// 조용히 기동한다 — 대시보드의 모든 `@/` 임포트가 죽는데 vitest는
// vitest.config.ts(자체 alias)를 써서 영향 밖이라 알람으로 성립한다.
// 생성은 전 tsconfig noEmit이 원천 차단하고(과거 composite emit이 원인),
// 로컬 잔존·커밋 유입(CI) 모두 이 테스트가 잡는다. 실패하면
// `rm webui/vite.config.js webui/vite.config.d.ts`.
import { describe, it, expect } from 'vitest';
import { existsSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const webuiRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..');

describe('낡은 vite.config 산출물 부재 게이트', () => {
  it.each(['vite.config.js', 'vite.config.d.ts'])(
    '%s가 존재하면 vite가 vite.config.ts 대신 그것을 집는다 — 삭제할 것',
    (f) => {
      expect(existsSync(resolve(webuiRoot, f)), `${f} — rm 후 재실행`).toBe(false);
    },
  );
});

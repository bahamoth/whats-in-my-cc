// vite.config.js/.d.ts 존재 게이트 (2026-07-06 실사고).
//
// tsc가 실수로 emit한 vite.config.js가 남으면 vite 설정 탐색이 .ts보다 .js를
// 먼저 집어(공식 docs: config resolution order) `@` alias 없는 낡은 설정으로
// 조용히 기동한다 — 대시보드의 모든 `@/` 임포트가 죽는데 vitest는
// vitest.config.ts(자체 alias)를 쓰고 CI는 깨끗한 체크아웃이라 어느 쪽도
// 원리상 못 잡는다. 두 파일은 커밋 금지 대상이라 .gitignore에 있고(git
// status에도 안 보임), 존재의 유일한 알람이 이 테스트다. 실패하면
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

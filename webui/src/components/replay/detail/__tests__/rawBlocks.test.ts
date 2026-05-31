import { describe, it, expect } from 'vitest';
import { buildRawBlocks } from '../rawBlocks';

const toolCallWithError = {
  node_id: 'n1',
  node_kind: 'tool_call',
  payload: {
    content_ordinal: 0,
    tool_name: 'Write',
    input: { file_path: '/a/x.ts' },
    result: {
      content_ordinal: 0,
      tool_result: {
        type: 'tool_result',
        content: '<tool_use_error>File has not been read yet.</tool_use_error>',
        is_error: true,
        tool_use_id: 'toolu_1',
      },
    },
  },
};

describe('buildRawBlocks', () => {
  it('surfaces the tool_result (error output) as its own block for a failed tool_call', () => {
    const blocks = buildRawBlocks(toolCallWithError, []);
    expect(blocks).not.toBeUndefined();
    const tr = blocks!.find((b) => b.source === 'tool_result');
    expect(tr).toBeDefined();
    expect(tr!.label).toBe('error'); // is_error: true
    // the actual error output text is present in the block record
    expect(JSON.stringify(tr!.record)).toContain('File has not been read yet');
  });

  it('labels the tool_result block "ok" when is_error is false', () => {
    const node = {
      ...toolCallWithError,
      payload: {
        ...toolCallWithError.payload,
        result: { tool_result: { content: 'done', is_error: false } },
      },
    };
    const tr = buildRawBlocks(node, [])!.find((b) => b.source === 'tool_result');
    expect(tr!.label).toBe('ok');
  });

  it('keeps the entity (call) block but without the duplicated result', () => {
    const blocks = buildRawBlocks(toolCallWithError, [])!;
    const entity = blocks[0];
    expect(entity.source).toBe('transcript');
    expect(entity.label).toBe('Write');
    expect(Object.keys(entity.record as object)).toContain('input');
    expect(Object.keys(entity.record as object)).not.toContain('result'); // split out
  });

  it('returns undefined for a plain message node with no facets and no tool_result (bare-record fallback)', () => {
    const node = { node_id: 'm1', node_kind: 'assistant_message', payload: { text: 'hi' } };
    expect(buildRawBlocks(node, [])).toBeUndefined();
  });

  it('includes folded facet blocks', () => {
    const node = { node_id: 'a1', node_kind: 'assistant_message', payload: { text: 'hi' } };
    const facets = [{ facet_kind: 'llm_request_span', data: { raw_span: { name: 'llm.request' } } }];
    const blocks = buildRawBlocks(node, facets)!;
    expect(blocks.some((b) => b.source === 'llm_request_span' && b.label === 'llm.request')).toBe(true);
  });
});

import { EditorSelection, type StateCommand } from "@codemirror/state";

/** Wraps each selection in `marker` (e.g. `**`), or unwraps if already wrapped. */
export function toggleWrap(marker: string, closing = marker): StateCommand {
  return ({ state, dispatch }) => {
    const changes = state.changeByRange((range) => {
      const before = state.sliceDoc(range.from - marker.length, range.from);
      const after = state.sliceDoc(range.to, range.to + closing.length);
      if (before === marker && after === closing) {
        return {
          changes: [
            { from: range.from - marker.length, to: range.from },
            { from: range.to, to: range.to + closing.length },
          ],
          range: EditorSelection.range(range.from - marker.length, range.to - marker.length),
        };
      }
      return {
        changes: [
          { from: range.from, insert: marker },
          { from: range.to, insert: closing },
        ],
        range: EditorSelection.range(range.from + marker.length, range.to + marker.length),
      };
    });
    dispatch(state.update(changes, { scrollIntoView: true, userEvent: "input" }));
    return true;
  };
}

const TASK = /^(\s*)([-*+]|\d+[.)]) \[([ xX])\] /;
const LIST = /^(\s*)([-*+]|\d+[.)]) /;

/** Cycles the current lines: plain → task, task ↔ done. */
export const toggleTask: StateCommand = ({ state, dispatch }) => {
  const lines = new Set<number>();
  for (const r of state.selection.ranges) {
    for (let n = state.doc.lineAt(r.from).number; n <= state.doc.lineAt(r.to).number; n++) lines.add(n);
  }
  const changes = [...lines].map((n) => {
    const line = state.doc.line(n);
    const task = TASK.exec(line.text);
    if (task) {
      const at = line.from + task[1].length + task[2].length + 2;
      return { from: at, to: at + 1, insert: task[3] === " " ? "x" : " " };
    }
    const list = LIST.exec(line.text);
    if (list) return { from: line.from + list[0].length, insert: "[ ] " };
    const indent = /^\s*/.exec(line.text)![0].length;
    return { from: line.from + indent, insert: "- [ ] " };
  });
  dispatch(state.update({ changes, userEvent: "input" }));
  return true;
};

/** Thin Node-API adapter: command forwarding, input batching, read-only metadata, subscriptions. */
import native from "./binding.cjs";
import type {
  Command,
  EditorState,
  InitialDocument,
  Layer,
  MaskMode,
  PaintTarget,
  Phase,
  Point,
  PointerSample,
  SelectionMode,
  Tool,
  Viewport,
  CanvasSizeOptions,
  CropRatio,
  MarqueeKind,
  PixelSelectionMode,
} from "./types.ts";
import type { NativeEditor } from "./native-api";
export type { SelectionMode, Tool } from "./types.ts";
export class Editor {
  private native: NativeEditor;
  private state: EditorState;
  private listeners = new Set<() => void>();
  private samples: PointerSample[] = [];
  private pointerTimer: ReturnType<typeof setTimeout> | undefined;
  private closed = false;
  constructor(options: InitialDocument = { kind: "demo" }, opened?: NativeEditor) {
    this.native = opened ?? new native.NativeEditor(JSON.stringify(options));
    this.state = JSON.parse(this.native.snapshot());
  }
  static async open(path: string) {
    return new Editor({ kind: "demo" }, await native.openEditor(path));
  }
  private accept(json: string) {
    this.state = JSON.parse(json);
    this.notify();
  }
  private dispatch(command: Command) {
    this.flushPointer();
    this.accept(this.native.dispatch(JSON.stringify(command)));
  }
  subscribe(listener: () => void) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }
  notify() {
    for (const listener of this.listeners) listener();
  }
  get document() {
    return this.state.document;
  }
  get history() {
    return this.state.history;
  }
  get selectedIds(): readonly string[] {
    return this.state.selection.ids;
  }
  get selectedId() {
    return this.state.selection.ids.at(-1);
  }
  get selected() {
    return this.document.layers.find((layer) => layer.id === this.selectedId);
  }
  get selectedLayers() {
    return this.document.layers.filter((layer) => this.isSelected(layer.id));
  }
  get layerRows() {
    return this.state.layerRows;
  }
  isSelected(id: string) {
    return this.selectedIds.includes(id);
  }
  get paintTarget() {
    return this.state.paintTarget;
  }
  get maskMode() {
    return this.state.maskMode;
  }
  get tool() {
    return this.state.tool;
  }
  get color() {
    return this.state.color;
  }
  get brushSize() {
    return this.state.brushSize;
  }
  set brushSize(size: number) {
    this.dispatch({ type: "setBrush", size, opacity: this.brushOpacity });
  }
  get brushOpacity() {
    return this.state.brushOpacity;
  }
  set brushOpacity(opacity: number) {
    this.dispatch({ type: "setBrush", size: this.brushSize, opacity });
  }
  get viewport() {
    return this.state.viewport;
  }
  get cropRect() {
    return this.state.cropRect;
  }
  get cropRatio() {
    return this.state.cropRatio;
  }
  get pixelSelectionBounds() {
    return this.state.pixelSelectionBounds;
  }
  get pixelSelectionFeather() {
    return this.state.pixelSelectionFeather;
  }
  get marqueeKind() {
    return this.state.marqueeKind;
  }
  get selectionMode() {
    return this.state.selectionMode;
  }
  get textEditRequests() {
    return this.state.textEditRequests;
  }
  set viewport(viewport: Viewport) {
    this.dispatch({ type: "setViewport", viewport });
  }
  select(id?: string, mode: SelectionMode = "replace") {
    this.dispatch({ type: "select", id, mode });
  }
  selectAll() {
    this.dispatch({ type: "selectAll" });
  }
  setTool(tool: Tool) {
    this.dispatch({ type: "setTool", tool });
  }
  fit() {
    this.dispatch({ type: "fit" });
  }
  zoom(zoom: number, point?: Point) {
    this.dispatch({ type: "zoom", zoom, point });
  }
  pan(delta: Point) {
    this.dispatch({ type: "pan", delta });
  }
  resizeViewport(width: number, height: number) {
    this.dispatch({ type: "resizeViewport", width, height });
  }
  setColor(color: string) {
    this.dispatch({ type: "setColor", color });
  }
  updateLayer(patch: Partial<Layer>) {
    this.dispatch({ type: "updateLayer", patch });
  }
  addPaintLayer() {
    this.dispatch({ type: "addPaintLayer" });
  }
  addGradient() {
    this.dispatch({ type: "addGradient" });
  }
  addGroup() {
    this.dispatch({ type: "addGroup" });
  }
  groupSelected() {
    this.dispatch({ type: "groupSelected" });
  }
  toggleGroupExpansion(id: string) {
    this.dispatch({ type: "toggleGroupExpansion", id });
  }
  moveToGroup(parentId?: string) {
    this.dispatch({ type: "moveToGroup", parentId });
  }
  duplicate() {
    this.dispatch({ type: "duplicate" });
  }
  remove() {
    this.dispatch({ type: "remove" });
  }
  reorder(direction: -1 | 1) {
    this.dispatch({ type: "reorder", direction });
  }
  reorderTo(targetId: string, side: "above" | "below") {
    this.dispatch({ type: "reorderTo", targetId, side });
  }
  nudge(delta: Point) {
    this.dispatch({ type: "nudge", delta });
  }
  async resizeCanvas(options: CanvasSizeOptions) {
    this.finishGesture();
    this.accept(await this.native.resizeCanvas(JSON.stringify(options)));
  }
  setCropRatio(ratio: CropRatio) {
    this.dispatch({ type: "setCropRatio", ratio });
  }
  commitCrop() {
    this.dispatch({ type: "commitCrop" });
  }
  cancelCrop() {
    this.dispatch({ type: "cancelCrop" });
  }
  setMarqueeKind(kind: MarqueeKind) {
    this.dispatch({ type: "setMarqueeKind", kind });
  }
  setSelectionMode(mode: PixelSelectionMode) {
    this.dispatch({ type: "setSelectionMode", mode });
  }
  editText(target: { id?: string; point?: Point }) {
    this.dispatch({ type: "editText", ...target });
  }
  pickUnder(point: Point) {
    this.dispatch({ type: "pickUnder", point });
  }
  beginPropertyEdit(label: string) {
    this.dispatch({ type: "beginPropertyEdit", label });
  }
  featherSelection(amount: number) {
    this.dispatch({ type: "featherSelection", amount });
  }
  deselectPixels() {
    this.dispatch({ type: "deselectPixels" });
  }
  selectAllPixels() {
    this.dispatch({ type: "selectAllPixels" });
  }
  clearSelectedPixels() {
    this.dispatch({ type: "clearSelectedPixels" });
  }
  addMask(base: MaskMode = "reveal") {
    this.dispatch({ type: "addMask", base });
  }
  setPaintTarget(target: PaintTarget) {
    this.dispatch({ type: "setPaintTarget", target });
  }
  setMaskMode(mode: MaskMode) {
    this.dispatch({ type: "setMaskMode", mode });
  }
  resetMask(base: MaskMode) {
    this.dispatch({ type: "resetMask", base });
  }
  removeMask() {
    this.dispatch({ type: "removeMask" });
  }
  toggleMaskLink() {
    this.dispatch({ type: "toggleMaskLink" });
  }
  undo() {
    this.dispatch({ type: "undo" });
  }
  redo() {
    this.dispatch({ type: "redo" });
  }
  finishGesture() {
    this.dispatch({ type: "finishGesture" });
  }
  cancelGesture() {
    this.dispatch({ type: "cancelGesture" });
  }
  pointer(phase: Phase, point: Point, modifiers: Partial<PointerSample["modifiers"]> = {}) {
    this.samples.push({
      phase,
      point,
      modifiers: {
        shift: modifiers.shift ?? false,
        alt: modifiers.alt ?? false,
        control: modifiers.control ?? false,
        meta: modifiers.meta ?? false,
      },
    });
    if (phase !== "move" || this.samples.length >= 256) this.flushPointer();
    else this.pointerTimer ??= setTimeout(() => this.flushPointer(), 8);
  }
  private flushPointer() {
    if (this.pointerTimer) clearTimeout(this.pointerTimer);
    this.pointerTimer = undefined;
    if (!this.samples.length) return;
    const samples = this.samples;
    this.samples = [];
    this.accept(
      this.native.dispatch(JSON.stringify({ type: "pointer", samples } satisfies Command)),
    );
  }
  async preview() {
    this.flushPointer();
    return this.native.preview();
  }
  async save(path: string) {
    this.finishGesture();
    this.accept(await this.native.save(path));
  }
  async export(path: string, format: "png" | "jpeg") {
    this.finishGesture();
    await this.native.exportImage(path, format === "jpeg");
  }
  async importImages(paths: readonly string[]) {
    this.finishGesture();
    this.accept(await this.native.importImages([...paths]));
  }
  async sampleColor(point: Point) {
    this.flushPointer();
    const color = await this.native.sampleColor(point.x, point.y);
    if (color && !this.closed) this.setColor(color);
  }
  close() {
    if (this.pointerTimer) clearTimeout(this.pointerTimer);
    this.samples = [];
    this.listeners.clear();
    this.closed = true;
    this.native.close();
  }
}

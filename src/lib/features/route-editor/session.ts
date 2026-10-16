import { RouteEditor } from './state';
import { routeEditorPorts } from '$lib/features/route-editor/client';
/** Mounted tabs and page returns share one original unresolved local-save receipt. */
export const routeEditor = new RouteEditor(routeEditorPorts);

import { AppSurfaces } from "./AppSurfaces";
import { useAppController } from "./appController";

export { firstToolPath } from "../state/toolDetailsViewModel";
export { newTaskStatusLabel, relativeTime, taskWorkingStatusLabel } from "./taskSurfaceHelpers";

export function App() {
  const controller = useAppController();
  return <AppSurfaces controller={controller} />;
}

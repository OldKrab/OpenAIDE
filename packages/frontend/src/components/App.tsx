import { AppSurfaces } from "./AppSurfaces";
import { useAppController } from "./appController";
import { RemoteDeviceAddedNotice } from "./RemoteDeviceAddedNotice";

export { firstToolPath } from "../state/toolDetailsViewModel";
export { newTaskStatusLabel, relativeTime, taskWorkingStatusLabel } from "./taskSurfaceHelpers";

export function App() {
  const controller = useAppController();
  return (
    <>
      <AppSurfaces controller={controller} />
      <RemoteDeviceAddedNotice
        devices={controller.view.settings.devices}
        onReview={() => controller.callbacks.navigation.openSettings(undefined, undefined, undefined, "devices")}
      />
    </>
  );
}

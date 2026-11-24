import { act, create, type ReactTestRenderer } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useSendAfterUploads, type SendAfterUploadsReport } from "./useSendAfterUploads";

type Props = {
  canSubmit: boolean;
  failedUploadIds: string[];
  report: SendAfterUploadsReport;
  submit: () => void;
  uploadsInFlight: number;
};

let controls: ReturnType<typeof useSendAfterUploads> | undefined;

function Harness(props: Props) {
  controls = useSendAfterUploads(props);
  return null;
}

describe("useSendAfterUploads", () => {
  beforeEach(() => {
    (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
    controls = undefined;
  });

  function render(props: Props) {
    let renderer: ReactTestRenderer | undefined;
    act(() => {
      renderer = create(<Harness {...props} />);
    });
    return {
      update: (next: Partial<Props>) => act(() => renderer?.update(<Harness {...props} {...next} />)),
      unmount: () => act(() => renderer?.unmount()),
    };
  }

  it("sends once uploads finish and logs the wait with its outcome", () => {
    const report = vi.fn<SendAfterUploadsReport>();
    const submit = vi.fn();
    const view = render({ canSubmit: false, failedUploadIds: [], report, submit, uploadsInFlight: 2 });

    act(() => controls?.hold());
    expect(controls?.waiting).toBe(true);
    view.update({ uploadsInFlight: 1 });
    expect(submit).not.toHaveBeenCalled();
    view.update({ canSubmit: true, uploadsInFlight: 0 });

    expect(submit).toHaveBeenCalledTimes(1);
    expect(controls?.waiting).toBe(false);
    expect(report).toHaveBeenNthCalledWith(1, "composer_send_after_uploads_start", { uploads_in_flight: 2 });
    expect(report).toHaveBeenNthCalledWith(2, "composer_send_after_uploads_end", expect.objectContaining({
      outcome: "sent",
      duration_ms: expect.any(Number),
    }));
  });

  it("ignores a failure that was visible before Send but stops on a new one", () => {
    const report = vi.fn<SendAfterUploadsReport>();
    const submit = vi.fn();
    const view = render({ canSubmit: false, failedUploadIds: ["old"], report, submit, uploadsInFlight: 1 });

    act(() => controls?.hold());
    view.update({ failedUploadIds: ["old"], uploadsInFlight: 1 });
    expect(controls?.waiting).toBe(true);
    view.update({ failedUploadIds: ["old", "new"], uploadsInFlight: 0, canSubmit: true });

    expect(submit).not.toHaveBeenCalled();
    expect(report).toHaveBeenLastCalledWith("composer_send_after_uploads_end", expect.objectContaining({ outcome: "upload_failed" }));
  });

  it("stops without sending when the Composer cannot send after the uploads", () => {
    const report = vi.fn<SendAfterUploadsReport>();
    const submit = vi.fn();
    const view = render({ canSubmit: false, failedUploadIds: [], report, submit, uploadsInFlight: 1 });

    act(() => controls?.hold());
    view.update({ uploadsInFlight: 0 });

    expect(submit).not.toHaveBeenCalled();
    expect(report).toHaveBeenLastCalledWith("composer_send_after_uploads_end", expect.objectContaining({ outcome: "blocked" }));
  });

  it("logs a held Send that ends with the Composer", () => {
    const report = vi.fn<SendAfterUploadsReport>();
    const view = render({ canSubmit: false, failedUploadIds: [], report, submit: vi.fn(), uploadsInFlight: 1 });

    act(() => controls?.hold());
    view.unmount();

    expect(report).toHaveBeenLastCalledWith("composer_send_after_uploads_end", expect.objectContaining({ outcome: "unmounted" }));
  });
});

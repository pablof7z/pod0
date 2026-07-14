@preconcurrency import AVFoundation
import os.log
import SwiftUI

// MARK: - SwiftUI wrapper

struct QRCodeScannerView: UIViewRepresentable {
    let onScanned: (String) -> Void

    func makeUIView(context: Context) -> QRCaptureView {
        let view = QRCaptureView()
        view.onScanned = { value in
            context.coordinator.deliver(value)
        }
        view.start()
        return view
    }

    func updateUIView(_ uiView: QRCaptureView, context: Context) {}

    func makeCoordinator() -> Coordinator { Coordinator(onScanned: onScanned) }

    final class Coordinator: NSObject {
        private let onScanned: (String) -> Void
        private var delivered = false

        init(onScanned: @escaping (String) -> Void) { self.onScanned = onScanned }

        func deliver(_ value: String) {
            guard !delivered else { return }
            delivered = true
            onScanned(value)
        }
    }
}

// MARK: - UIKit capture view

final class QRCaptureView: UIView {
    private let logger = Logger.app("QRCodeScannerView")
    var onScanned: ((String) -> Void)?

    nonisolated(unsafe) private let session = AVCaptureSession()
    private var previewLayer: AVCaptureVideoPreviewLayer?

    override class var layerClass: AnyClass { AVCaptureVideoPreviewLayer.self }

    func start() {
        guard let device = AVCaptureDevice.default(for: .video) else {
            logger.error("QRCodeScannerView: no video capture device available")
            return
        }
        let input: AVCaptureDeviceInput
        do {
            input = try AVCaptureDeviceInput(device: device)
        } catch {
            logger.error("QRCodeScannerView: failed to create capture input: \(error, privacy: .public)")
            return
        }
        guard session.canAddInput(input) else {
            logger.error("QRCodeScannerView: session cannot add video input")
            return
        }

        session.addInput(input)

        let output = AVCaptureMetadataOutput()
        guard session.canAddOutput(output) else { return }
        session.addOutput(output)
        output.setMetadataObjectsDelegate(self, queue: .main)
        output.metadataObjectTypes = [.qr]

        if let layer = layer as? AVCaptureVideoPreviewLayer {
            layer.session = session
            layer.videoGravity = .resizeAspectFill
            previewLayer = layer
        }

        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.session.startRunning()
        }
    }

    func stop() {
        DispatchQueue.global(qos: .userInitiated).async { [weak self] in
            self?.session.stopRunning()
        }
    }

    deinit {
        let s = session
        DispatchQueue.global(qos: .userInitiated).async { s.stopRunning() }
    }
}

extension QRCaptureView: @preconcurrency AVCaptureMetadataOutputObjectsDelegate {
    func metadataOutput(_ output: AVCaptureMetadataOutput, didOutput objects: [AVMetadataObject], from connection: AVCaptureConnection) {
        guard let object = objects.first as? AVMetadataMachineReadableCodeObject,
              let value = object.stringValue else { return }
        onScanned?(value)
    }
}

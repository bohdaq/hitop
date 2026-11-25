import UIKit
import UniformTypeIdentifiers

/// "HITOP" in the share sheet: keeps what was shared for the app, which takes it when it opens (an
/// extension can't open its app). Nothing is sent anywhere.
final class ShareViewController: UIViewController {
    private let label = UILabel()
    private let detail = UILabel()
    private let done = UIButton(configuration: .borderedProminent())
    private var started = false

    override func viewDidLoad() {
        super.viewDidLoad()
        overrideUserInterfaceStyle = .dark
        view.backgroundColor = .systemBackground
        label.font = .preferredFont(forTextStyle: .headline)
        detail.font = .preferredFont(forTextStyle: .subheadline)
        detail.textColor = .secondaryLabel
        for l in [label, detail] {
            l.adjustsFontForContentSizeCategory = true
            l.numberOfLines = 0
            l.textAlignment = .center
        }
        label.text = "Adding to HITOP…"
        done.setTitle("Done", for: .normal)
        done.addAction(UIAction { [weak self] _ in self?.extensionContext?.completeRequest(returningItems: nil) }, for: .touchUpInside)
        let stack = UIStackView(arrangedSubviews: [label, detail, done])
        stack.axis = .vertical
        stack.spacing = 16
        stack.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(stack)
        NSLayoutConstraint.activate([
            stack.centerYAnchor.constraint(equalTo: view.centerYAnchor),
            stack.leadingAnchor.constraint(equalTo: view.layoutMarginsGuide.leadingAnchor, constant: 8),
            stack.trailingAnchor.constraint(equalTo: view.layoutMarginsGuide.trailingAnchor, constant: -8),
            done.heightAnchor.constraint(greaterThanOrEqualToConstant: 48),
        ])
    }

    override func viewDidAppear(_ animated: Bool) {
        super.viewDidAppear(animated)
        guard !started else { return }
        started = true
        keep()
    }

    private func keep() {
        guard let root = FileManager.default.containerURL(forSecurityApplicationGroupIdentifier: "group.hitop.rws8.tech.HiTop") else {
            return show("HITOP can't take this now", "Its shared folder isn't there.")
        }
        let inbox = SharedInbox(root: root)
        let providers = (extensionContext?.inputItems ?? []).compactMap { $0 as? NSExtensionItem }.flatMap { $0.attachments ?? [] }
        let finish: @Sendable ((any Error)?) -> Void = { [weak self] error in
            DispatchQueue.main.async {
                if let error { self?.show("HITOP can't take this", error.localizedDescription) } else { self?.show("Added", "Open HITOP to look at it and add it.") }
            }
        }
        // A web address before its page's text, a file before its text.
        if let p = providers.first(where: { $0.hasItemConformingToTypeIdentifier(UTType.url.identifier) && !$0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) }) {
            _ = p.loadObject(ofClass: URL.self) { url, error in
                finish(Self.attempt(error) { if let url { try inbox.add(text: url.absoluteString) } })
            }
        } else if let p = providers.first(where: { $0.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) || ($0.suggestedName != nil && $0.hasItemConformingToTypeIdentifier(UTType.data.identifier)) }) {
            let type = p.registeredTypeIdentifiers.first { $0 != UTType.fileURL.identifier } ?? UTType.data.identifier
            let name = p.suggestedName
            _ = p.loadFileRepresentation(forTypeIdentifier: type) { url, error in
                // The file is there until this returns.
                finish(Self.attempt(error) {
                    if let url { try inbox.add(file: url, name: name.map { url.pathExtension.isEmpty || $0.hasSuffix(".\(url.pathExtension)") ? $0 : "\($0).\(url.pathExtension)" }) }
                })
            }
        } else if let p = providers.first(where: { $0.canLoadObject(ofClass: String.self) }) {
            _ = p.loadObject(ofClass: String.self) { text, error in
                finish(Self.attempt(error) { if let text { try inbox.add(text: text) } })
            }
        } else {
            show("HITOP can't take this", "It takes text, addresses and files.")
        }
    }

    nonisolated private static func attempt(_ error: (any Error)?, _ work: () throws -> Void) -> (any Error)? {
        if let error { return error }
        do { try work(); return nil } catch { return error }
    }

    private func show(_ title: String, _ text: String) {
        label.text = title
        detail.text = text
        UIAccessibility.post(notification: .screenChanged, argument: label)
    }
}

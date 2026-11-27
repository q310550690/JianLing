import UIKit
import WebKit

final class WebViewController: UIViewController, WKNavigationDelegate {
    private var webView: WKWebView!

    override func viewDidLoad() {
        super.viewDidLoad()
        view.backgroundColor = .white

        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        if #available(iOS 10.0, *) {
            config.mediaTypesRequiringUserActionForPlayback = []
        }

        webView = WKWebView(frame: view.bounds, configuration: config)
        webView.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        webView.navigationDelegate = self
        view.addSubview(webView)

        loadStartPage()
    }

    private func loadStartPage() {
        if AppConfig.useLocalContent,
           let index = Bundle.main.url(
               forResource: "index",
               withExtension: "html",
               subdirectory: "www"
           ) {
            webView.loadFileURL(index, allowingReadAccessTo: index.deletingLastPathComponent())
            return
        }
        if let url = URL(string: AppConfig.startUrl) {
            webView.load(URLRequest(url: url))
        }
    }
}

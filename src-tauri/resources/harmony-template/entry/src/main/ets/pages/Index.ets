import { webview } from '@kit.ArkWeb';
import { AppConfig } from './AppConfig';

@Entry
@Component
struct Index {
  controller: webview.WebviewController = new webview.WebviewController();

  private pageSrc(): string | Resource {
    return AppConfig.local ? $rawfile('www/index.html') : AppConfig.entry;
  }

  build() {
    Column() {
      Web({ src: this.pageSrc(), controller: this.controller })
        .javaScriptAccess(true)
        .domStorageAccess(true)
        .fileAccess(true)
        .imageAccess(true)
        .zoomAccess(AppConfig.zoom)
        .width('100%')
        .height('100%')
    }
    .width('100%')
    .height('100%')
  }
}

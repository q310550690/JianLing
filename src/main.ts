import { createApp } from "vue";
import ElementPlus from "element-plus";
import zhCn from "element-plus/es/locale/lang/zh-cn";
import "element-plus/dist/index.css";
import "./styles.css";
import App from "./App.vue";
import router from "./router";

createApp(App).use(router).use(ElementPlus, { locale: zhCn }).mount("#app");

// 启动图：首帧渲染完成后淡出并移除（index.html 里的内联层）
const splash = document.getElementById("boot-splash");
if (splash) {
  requestAnimationFrame(() => {
    splash.style.opacity = "0";
    window.setTimeout(() => splash.remove(), 280);
  });
}

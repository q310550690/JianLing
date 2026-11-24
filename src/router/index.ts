import { createRouter, createWebHashHistory } from "vue-router";
import ProjectsView from "../views/ProjectsView.vue";
import ProjectView from "../views/ProjectView.vue";
import ProjectDebugView from "../views/ProjectDebugView.vue";
import SettingsView from "../views/SettingsView.vue";
import DefaultLayout from "../layouts/DefaultLayout.vue";

export const router = createRouter({
  history: createWebHashHistory(),
  routes: [
    {
      path: "/",
      component: DefaultLayout,
      children: [
        { path: "", redirect: "/projects" },
        {
          path: "projects",
          name: "projects",
          component: ProjectsView,
          meta: { title: "项目管理", keepAlive: true },
        },
        {
          path: "projects/:id",
          name: "project",
          component: ProjectView,
          props: (route) => ({ projectId: String(route.params.id ?? "") }),
          meta: { title: "套壳配置" },
        },
        {
          path: "projects/:id/debug",
          name: "project-debug",
          component: ProjectDebugView,
          props: (route) => ({ projectId: String(route.params.id ?? "") }),
          meta: { title: "调试包" },
        },
        {
          path: "settings",
          name: "settings",
          component: SettingsView,
          meta: { title: "设置", keepAlive: true },
        },
      ],
    },
  ],
});

export default router;

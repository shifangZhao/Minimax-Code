import { createApp } from "vue";
import App from "./App.vue";
import router from "./router";
import VueVirtualScroller from "vue-virtual-scroller";
import "vue-virtual-scroller/dist/vue-virtual-scroller.css";
import "./assets/main.css";

const app = createApp(App)
app.use(router)
app.use(VueVirtualScroller)
app.mount("#app");
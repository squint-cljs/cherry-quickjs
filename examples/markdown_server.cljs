;; markdown preview server: hono + markdown-it from esm.sh
;; run with: cherry-quickjs examples/markdown_server.cljs
;; then: curl -d '# hi' http://localhost:3000
(require '["https://esm.sh/hono" :refer [Hono]]
         '["https://esm.sh/markdown-it@14.1.0$default" :as MarkdownIt]
         '[cherry.http :refer [serve]])

(def md (MarkdownIt.))
(def app (Hono.))

(.post app "/" (fn [c] (.then (.text (.-req c)) (fn [b] (.html c (.render md b))))))

(serve (.-fetch app) {:port 3000})

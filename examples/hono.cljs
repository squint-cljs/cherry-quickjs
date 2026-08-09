;; web server with hono from esm.sh
;; run with: choq examples/hono.cljs
(require '["https://esm.sh/hono" :refer [Hono]]
         '[cherry.http :refer [serve]])

(def app (Hono.))

(.get app "/" (fn [c] (.text c "hello from hono")))
(.get app "/json" (fn [c] (.json c #js {:framework "hono" :runtime "choq"})))
(.post app "/echo" (fn [c] (.then (.text (.-req c)) (fn [b] (.text c b)))))

(serve (.-fetch app) {:port 3000})

;; server-side rendering with preact from esm.sh
;; run with: choq examples/ssr.cljs
(require '["https://esm.sh/preact-render-to-string@6.5.11?deps=preact@10.24.3" :refer [render]]
         '["https://esm.sh/preact@10.24.3" :refer [h]])

(defn todo-list [items]
  (h "ul" #js {:class "todos"}
     (into-array (map #(h "li" nil %) items))))

(defn page [items]
  (h "html" nil
     (into-array
      [(h "head" nil (h "title" nil "todos"))
       (h "body" nil (todo-list items))])))

(println (render (page ["write examples" "push to github"])))

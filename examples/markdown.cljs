;; render markdown to html with marked from esm.sh
;; run with: cherry-quickjs examples/markdown.cljs
(require '["https://esm.sh/marked@16.4.1" :refer [marked]])

(def md "# Hello cherry\n\nSome *emphasis* and a [link](https://github.com/squint-cljs/cherry).\n\n- one\n- two\n")

(println (marked md))

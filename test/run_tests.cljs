;; runs all test files in one engine, run with: choq test/run_tests.cljs
(require '["node:fs" :as fs]
         '[clojure.test :as t])

(set! (.-__testRunner js/globalThis) true)

(def files ["test/nrepl_test.cljs"
            "test/stream_test.cljs"
            "test/url_import_test.cljs"])

(defn ^:async load-tests [fs-files]
  (doseq [f fs-files]
    (println "Loading" f)
    (let [[status payload] (await (js/__evalCherry (fs/readFileSync f "utf8")))]
      (when (= "error" status)
        (println "Error loading" f ":" payload)
        (js/process.exit 1)))))

#_:clj-kondo/ignore
(await (load-tests files))

(let [summary (t/run-tests)]
  (js/process.exit (if (pos? (+ (:fail summary) (:error summary))) 1 0)))

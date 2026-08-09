;; smoke test for node:stream, run with: choq test/stream_test.cljs
(require '["node:stream" :refer [PassThrough Readable Writable]]
         '[clojure.test :as t :refer [deftest is testing]])

(defn collect
  "Resolves to a vector of string chunks read from stream."
  [stream]
  (js/Promise.
   (fn [resolve reject]
     (let [chunks (atom [])]
       (.on stream "data" (fn [d] (swap! chunks conj (.toString d))))
       (.on stream "end" (fn [] (resolve @chunks)))
       (.on stream "error" reject)))))

#_:clj-kondo/ignore
(def read-result (await (collect (Readable.from #js ["a" "b" "c"]))))

#_:clj-kondo/ignore
(def pipe-result
  (await
   (js/Promise.
    (fn [resolve reject]
      (let [out (atom [])
            w (Writable. #js {:write (fn [chunk _enc cb]
                                       (swap! out conj (.toString chunk))
                                       (cb))})]
        (.on w "finish" (fn [] (resolve @out)))
        (.on w "error" reject)
        (.pipe (Readable.from #js ["x" "y"]) w))))))

(def pt (PassThrough.))
(.end pt "hello")
#_:clj-kondo/ignore
(def passthrough-result (await (collect pt)))

(deftest readable-test
  (testing "Readable.from emits data and end"
    (is (= ["a" "b" "c"] read-result))))

(deftest pipe-test
  (testing "pipe drives a Writable to finish"
    (is (= ["x" "y"] pipe-result))))

(deftest passthrough-test
  (testing "PassThrough passes written data through"
    (is (= ["hello"] passthrough-result))))

(def summary (t/run-tests))
(js/process.exit (if (pos? (+ (:fail summary) (:error summary))) 1 0))

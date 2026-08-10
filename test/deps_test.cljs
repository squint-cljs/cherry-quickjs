;; dynamic dependency test; loaded by test/run_tests.cljs
(require '[choq.deps :as deps]
         '[clojure.test :as t :refer [deftest is testing]])

(deps/add-deps '{:deps {dev.weavejester/medley {:mvn/version "1.8.0"}}})

(require '[medley.core :as m])

(deftest add-deps-test
  (testing "a clojars lib resolves and loads"
    (is (= {1 {:id 1} 2 {:id 2}} (m/index-by :id [{:id 1} {:id 2}])))
    (is (= {:a 2 :b 3} (m/map-vals inc {:a 1 :b 2})))))

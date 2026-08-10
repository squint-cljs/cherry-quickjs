;; dynamic dependency test; loaded by test/run_tests.cljs
(require '[choq.deps :as deps]
         '[clojure.test :as t :refer [deftest is testing]])

(deps/add-deps '{:deps {dev.weavejester/medley {:mvn/version "1.8.0"}}})

(require '[medley.core :as m])

(require '["mvn:babashka/fs@0.5.34/babashka.fs" :as bfs-mvn])
(require '["mvn:dev.weavejester/medley@1.8.0/medley.core" :as mvn-m])

(deftest add-deps-test
  (testing "a clojars lib resolves and loads"
    (is (= {1 {:id 1} 2 {:id 2}} (m/index-by :id [{:id 1} {:id 2}])))
    (is (= {:a 2 :b 3} (m/map-vals inc {:a 1 :b 2})))))

(deftest babashka-fs-mvn-test
  (testing "babashka.fs from clojars works without a jvm"
    (is (true? (bfs-mvn/exists? "README.md")))
    (is (= "# Choq" (subs (bfs-mvn/slurp "README.md") 0 6)))))

(deftest mvn-specifier-test
  (testing "a mvn: specifier resolves and requires in one step"
    (is (= {:a 2} (mvn-m/map-vals inc {:a 1})))))
